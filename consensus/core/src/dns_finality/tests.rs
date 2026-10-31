//! **`dns_finality`'s `tests` module**, moved out of `dns_finality.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).

use super::*;
use crate::palw_state_v2::{PALW_V2_MAX_DEFERRED_VALIDATOR_PAYOUTS, PALW_V2_MAX_RESERVE_DRIP_PAYOUTS};

// ---- ADR-0022: OverlaySnapshot commitment ----

fn mk_bond(seed: u64, amount: u64) -> StakeBondRecord {
    StakeBondRecord {
        version: 2,
        bond_outpoint: TransactionOutpoint::new(Hash64::from_u64_word(seed), (seed % 7) as u32),
        owner_pubkey_hash: Hash64::from_u64_word(seed + 1),
        validator_pubkey_hash: Hash64::from_u64_word(seed + 2),
        validator_pubkey: vec![(seed % 251) as u8; 8],
        amount,
        activation_daa_score: seed,
        created_daa_score: seed,
        unbonding_period_blocks: 700,
        owner_reward_spk_payload: [(seed % 256) as u8; 64],
        unbond_request_daa_score: None,
        slashed_at_daa_score: None,
        status: BondStatus::Active,
    }
}

fn mk_contrib(seed: u64) -> BlockOverlayContribution {
    let op = |s: u64| TransactionOutpoint::new(Hash64::from_u64_word(s), 0);
    BlockOverlayContribution {
        block_hash: Hash64::from_u64_word(seed),
        block_daa_score: seed * 100,
        rewarded_keys: vec![(op(seed + 11), 3u64), (op(seed + 10), 2u64)],
        quality_subpool: seed,
    }
}

/// kaspa-pq GetStakeBonds: owner filtering, deterministic outpoint ordering,
/// and cursor paging reproduce the full ordered set exactly (no gaps/dupes).
#[test]
fn paginate_filters_orders_and_pages() {
    let owner_a = Hash64::from_u64_word(1000);
    let owner_b = Hash64::from_u64_word(2000);
    let mut records = Vec::new();
    for seed in [5u64, 1, 9, 3, 7] {
        let mut b = mk_bond(seed, seed * 10);
        b.owner_pubkey_hash = owner_a;
        records.push(b);
    }
    for seed in [2u64, 8] {
        let mut b = mk_bond(seed, seed * 10);
        b.owner_pubkey_hash = owner_b;
        records.push(b);
    }
    let pov = 100;
    let query =
        |cursor, limit| StakeBondQuery { owner_pubkey_hash: Some(owner_a), status_in: None, cursor, limit, pov_daa_score: None };

    // Owner filter isolates A's 5 bonds; a fits-in-one-page result has no cursor.
    let all_a = paginate_stake_bonds(records.clone(), &query(None, 0), pov);
    assert_eq!(all_a.bonds.len(), 5);
    assert!(all_a.bonds.iter().all(|b| b.owner_pubkey_hash == owner_a));
    assert!(all_a.next_cursor.is_none());
    assert_eq!(all_a.pov_daa_score, pov);

    // Deterministic ascending order by (txid, index).
    let mut sorted = all_a.bonds.clone();
    sorted.sort_by(|a, b| {
        (a.bond_outpoint.transaction_id, a.bond_outpoint.index).cmp(&(b.bond_outpoint.transaction_id, b.bond_outpoint.index))
    });
    assert_eq!(all_a.bonds, sorted, "page must be ordered by outpoint");

    // Paging with limit=2 walks the same sequence via next_cursor — exclusive,
    // so no gaps and no duplicates.
    let mut paged = Vec::new();
    let mut cursor = None;
    loop {
        let page = paginate_stake_bonds(records.clone(), &query(cursor, 2), pov);
        assert!(page.bonds.len() <= 2);
        paged.extend(page.bonds.iter().cloned());
        match page.next_cursor {
            Some(c) => {
                assert_eq!(page.bonds.len(), 2, "a non-final page is full");
                cursor = Some(c);
            }
            None => break,
        }
    }
    assert_eq!(paged, all_a.bonds, "paged walk must reproduce the full ordered set exactly");
}

/// kaspa-pq GetStakeBonds: the status filter uses *effective* status at the
/// pov DAA score (not the stored field); an empty list means "any status".
#[test]
fn paginate_filters_by_effective_status() {
    let pov = 50;
    let mut pending = mk_bond(200, 1); // activation 200 > pov → Pending
    pending.status = BondStatus::Pending;
    let active = mk_bond(10, 2); // activation 10 ≤ pov, no unbond/slash → Active
    let mut unbonding = mk_bond(20, 3);
    unbonding.unbond_request_daa_score = Some(30); // ≤ pov → Unbonding
    let mut slashed = mk_bond(31, 4);
    slashed.slashed_at_daa_score = Some(40); // ≤ pov → Slashed
    let records = vec![pending, active.clone(), unbonding.clone(), slashed];

    let q =
        |statuses| StakeBondQuery { owner_pubkey_hash: None, status_in: Some(statuses), cursor: None, limit: 0, pov_daa_score: None };

    let only_active = paginate_stake_bonds(records.clone(), &q(vec![BondStatus::Active]), pov);
    assert_eq!(only_active.bonds.len(), 1);
    assert_eq!(only_active.bonds[0].bond_outpoint, active.bond_outpoint);

    let two = paginate_stake_bonds(records.clone(), &q(vec![BondStatus::Active, BondStatus::Unbonding]), pov);
    assert_eq!(two.bonds.len(), 2);
    assert!(two.bonds.iter().any(|b| b.bond_outpoint == unbonding.bond_outpoint));

    // Empty status set is treated as no status filter (all four records).
    let any = paginate_stake_bonds(records, &q(vec![]), pov);
    assert_eq!(any.bonds.len(), 4);
}

/// kaspa-pq GetStakeBonds: under a FIXED pov (the pinning the request's
/// `pov_daa_score` provides), a `status_in`-filtered multi-page walk is
/// gap-free — the property that prevents the mid-walk skip when the sink
/// would otherwise advance between page fetches.
#[test]
fn paginate_status_filtered_walk_is_complete_under_fixed_pov() {
    let pov = 100;
    let owner = Hash64::from_u64_word(4242);
    let mut records = Vec::new();
    for seed in [5u64, 1, 9, 3, 7] {
        // activation = seed ≤ pov → Active
        let mut b = mk_bond(seed, seed);
        b.owner_pubkey_hash = owner;
        records.push(b);
    }
    for seed in [200u64, 150] {
        // activation > pov → Pending (must be excluded by status=Active)
        let mut b = mk_bond(seed, seed);
        b.owner_pubkey_hash = owner;
        records.push(b);
    }
    let q = |cursor, limit| StakeBondQuery {
        owner_pubkey_hash: Some(owner),
        status_in: Some(vec![BondStatus::Active]),
        cursor,
        limit,
        pov_daa_score: Some(pov),
    };

    let full = paginate_stake_bonds(records.clone(), &q(None, 0), pov);
    assert_eq!(full.bonds.len(), 5, "5 active bonds match at the fixed pov");

    let mut paged = Vec::new();
    let mut cursor = None;
    loop {
        let page = paginate_stake_bonds(records.clone(), &q(cursor, 2), pov);
        paged.extend(page.bonds.iter().cloned());
        match page.next_cursor {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }
    assert_eq!(paged, full.bonds, "fixed-pov status-filtered walk reproduces the full set with no gap");
}

/// ADR-0022: the overlay commitment is order-independent (canonicalized) and
/// sensitive to every component — the load-bearing property for verifying a
/// peer-supplied snapshot against `header.overlay_commitment_root`.
#[test]
fn overlay_commitment_is_canonical_and_sensitive() {
    let bonds = vec![mk_bond(9, 100), mk_bond(2, 200), mk_bond(5, 300)];
    let window = vec![mk_contrib(40), mk_contrib(20), mk_contrib(31)];
    let a = OverlaySnapshot { bonds: bonds.clone(), reserve_balance: 12345, window: window.clone() };
    // Same content, every vector reversed → same commitment after canonicalization.
    let b = OverlaySnapshot {
        bonds: bonds.iter().rev().cloned().collect(),
        reserve_balance: 12345,
        window: window
            .iter()
            .rev()
            .map(|c| {
                let mut c = c.clone();
                c.rewarded_keys.reverse();
                c
            })
            .collect(),
    };
    assert_eq!(a.commitment_root(), b.commitment_root(), "commitment must be order-independent");

    // Every component is committed: changing any one flips the digest.
    let mut c = a.clone();
    c.reserve_balance += 1;
    assert_ne!(a.commitment_root(), c.commitment_root(), "reserve_balance must be committed");
    let mut d = a.clone();
    d.bonds[0].amount += 1;
    assert_ne!(a.commitment_root(), d.commitment_root(), "bond amount must be committed");
    let mut e = a.clone();
    e.window[0].quality_subpool += 1;
    assert_ne!(a.commitment_root(), e.commitment_root(), "quality sub-pool must be committed");
    let mut f = a.clone();
    f.window[0].rewarded_keys[0].1 += 1;
    assert_ne!(a.commitment_root(), f.commitment_root(), "rewarded keys must be committed");
    let mut g = a.clone();
    g.window[0].block_daa_score += 1;
    assert_ne!(a.commitment_root(), g.commitment_root(), "block daa score must be committed");

    // epoch_contributions is daa-ordered regardless of window order.
    let contribs = a.epoch_contributions();
    assert!(contribs.windows(2).all(|w| w[0].block_daa_score <= w[1].block_daa_score));

    // The empty snapshot has a stable, non-zero digest (the genesis/pre-bond value).
    assert_ne!(OverlaySnapshot::default().commitment_root(), Hash64::default());
}

// ---- PR-10.5: StakeScore + DNS reorg gate ----

#[test]
fn epoch_stake_credit_quality_floor() {
    // φS = 0 reproduces the prior linear credit EXACTLY (the 10_000 factors cancel).
    assert_eq!(epoch_stake_credit(1, 3, 0), 333_333_333); // floor(1e9/3)
    assert_eq!(epoch_stake_credit(5, 10, 0), STAKE_SCORE_SCALE / 2); // 0.5
    assert_eq!(epoch_stake_credit(50, 50, 0), STAKE_SCORE_SCALE); // 1.0
    // φS = 0.60: below → 0, exactly-at → 0 (continuous), above → smooth (f−φS)/(1−φS).
    let floor = 6000u16;
    assert_eq!(epoch_stake_credit(5, 10, floor), 0); // 0.50 < 0.60
    assert_eq!(epoch_stake_credit(6, 10, floor), 0); // exactly at the floor → 0
    assert_eq!(epoch_stake_credit(8, 10, floor), STAKE_SCORE_SCALE / 2); // (0.80−0.60)/0.40 = 0.5
    assert_eq!(epoch_stake_credit(10, 10, floor), STAKE_SCORE_SCALE); // full inclusion → 1.0
    // Edges.
    assert_eq!(epoch_stake_credit(7, 0, floor), 0); // no active stake
    assert_eq!(epoch_stake_credit(999, 50, floor), STAKE_SCORE_SCALE); // numerator clamped to expected
    assert_eq!(epoch_stake_credit(10, 10, 10_000), STAKE_SCORE_SCALE); // φS = 100%: full inclusion only
    assert_eq!(epoch_stake_credit(9, 10, 10_000), 0); // φS = 100%: < 100% → 0
}

#[test]
fn compute_stake_score_sums_credits_deterministically() {
    let epochs = vec![
        EpochStakeTally { epoch: 1, signed_weight: 10, total_weight: 10 }, // 1.0
        EpochStakeTally { epoch: 2, signed_weight: 5, total_weight: 10 },  // 0.5
        EpochStakeTally { epoch: 3, signed_weight: 0, total_weight: 10 },  // 0.0
    ];
    // φS = 0 (no floor): linear sum 1.0 + 0.5 + 0.0.
    let s = compute_stake_score(&epochs, EpochCreditRule::QualityFloor { quality_floor_bps: 0 });
    assert_eq!(s, StakeScore(STAKE_SCORE_SCALE + STAKE_SCORE_SCALE / 2));
    assert_eq!(compute_stake_score(&epochs, EpochCreditRule::QualityFloor { quality_floor_bps: 0 }), s); // deterministic
    assert_eq!(compute_stake_score(&[], EpochCreditRule::QualityFloor { quality_floor_bps: 0 }), StakeScore(0));
    // φS = 0.60: epoch 2 (0.50) and 3 (0.0) drop to 0; only epoch 1 (1.0) credits.
    assert_eq!(compute_stake_score(&epochs, EpochCreditRule::QualityFloor { quality_floor_bps: 6000 }), StakeScore(STAKE_SCORE_SCALE));
}

#[test]
fn derive_dns_health_signal() {
    let tally = |signed: u128, total: u128| EpochStakeTally { epoch: 0, signed_weight: signed, total_weight: total };
    let (floor, censor, m) = (EpochCreditRule::QualityFloor { quality_floor_bps: 6000 }, 1000u16, 3u32);
    // Not active → DisabledBeforeActivation regardless of tallies.
    assert_eq!(derive_dns_health(&[tally(0, 10)], floor, censor, m, false), DnsHealth::DisabledBeforeActivation);
    // Fewer than M epochs of history → Active (no sustained signal yet).
    assert_eq!(derive_dns_health(&[tally(0, 10), tally(0, 10)], floor, censor, m, true), DnsHealth::Active);
    // A recent epoch meets φS → Active even after a dip (health recovers immediately).
    let recovered = vec![tally(0, 10), tally(0, 10), tally(7, 10)]; // last = 0.70 ≥ 0.60
    assert_eq!(derive_dns_health(&recovered, floor, censor, m, true), DnsHealth::Active);
    // Last M all below φS but above the censorship floor → StakeQualityLow.
    let low = vec![tally(3, 10), tally(2, 10), tally(4, 10)]; // 0.30 / 0.20 / 0.40, in [0.10, 0.60)
    assert_eq!(derive_dns_health(&low, floor, censor, m, true), DnsHealth::DegradedStakeQualityLow);
    // Last M all near-zero (below the censorship floor) → CertificateCensored.
    assert_eq!(
        derive_dns_health(&[tally(0, 10), tally(0, 10), tally(0, 10)], floor, censor, m, true),
        DnsHealth::DegradedCertificateCensored
    );
    // Not ALL below the censorship floor within the window → StakeQualityLow.
    let mixed = vec![tally(0, 10), tally(0, 10), tally(3, 10)]; // last 0.30 > 0.10 censorship floor
    assert_eq!(derive_dns_health(&mixed, floor, censor, m, true), DnsHealth::DegradedStakeQualityLow);
}

#[test]
fn is_dns_confirmed_requires_both_thresholds() {
    let w = BlueWorkType::from_u64;
    let (cw, cs) = (w(100), StakeScore(STAKE_SCORE_SCALE));
    assert!(is_dns_confirmed(w(100), StakeScore(STAKE_SCORE_SCALE), cw, cs)); // both met
    assert!(is_dns_confirmed(w(200), StakeScore(STAKE_SCORE_SCALE * 2), cw, cs));
    assert!(!is_dns_confirmed(w(99), StakeScore(STAKE_SCORE_SCALE), cw, cs)); // work short
    assert!(!is_dns_confirmed(w(100), StakeScore(STAKE_SCORE_SCALE - 1), cw, cs)); // stake short
}

fn reorg_inputs(stage: DnsRolloutStage, mode: DnsReorgMode, includes: bool, cw: u64, kw: u64, cs: u128, ks: u128) -> DnsReorgInputs {
    DnsReorgInputs {
        rollout_stage: stage,
        mode,
        candidate_includes_confirmed_anchor: includes,
        // Override disabled by default so these cases exercise the strict 2-D rule exactly as
        // before; the override has its own dedicated tests below.
        emergency_work_override_multiplier: 0,
        candidate_work_after: BlueWorkType::from_u64(cw),
        canonical_work_after: BlueWorkType::from_u64(kw),
        candidate_stake_after: StakeScore(cs),
        canonical_stake_after: StakeScore(ks),
        emergency_work_margin: BlueWorkType::from_u64(0),
        emergency_stake_margin: StakeScore(0),
        // A fresh anchor (age 0) with the TTL disabled: these cases exercise the dominance
        // rule, not the staleness release, which has its own tests below.
        confirmed_anchor_age_daa_score: 0,
        veto_ttl_daa_score: u64::MAX,
    }
}

#[test]
fn dns_reorg_gate_dormant_before_active() {
    let i = reorg_inputs(DnsRolloutStage::Bootstrap, DnsReorgMode::TwoDimensionalDominance, false, 1, 100, 1, 100);
    assert_eq!(check_dns_reorg_rule(&i), DnsReorgOutcome::GateInactive);
    assert!(check_dns_reorg_rule(&i).is_accept());
}

#[test]
fn dns_reorg_includes_confirmed_anchor_ok() {
    let i = reorg_inputs(DnsRolloutStage::Active, DnsReorgMode::TwoDimensionalDominance, true, 0, 999, 0, 999);
    assert_eq!(check_dns_reorg_rule(&i), DnsReorgOutcome::IncludesConfirmedAnchor);
}

#[test]
fn dns_reorg_hard_checkpoint_rejects_any_exit() {
    // Even a candidate that dominates on both axes is rejected under hard-checkpoint.
    let i = reorg_inputs(DnsRolloutStage::Active, DnsReorgMode::HardCheckpoint, false, 9999, 1, 9999, 1);
    assert_eq!(check_dns_reorg_rule(&i), DnsReorgOutcome::HardCheckpointReject);
    assert!(!check_dns_reorg_rule(&i).is_accept());
}

#[test]
fn dns_reorg_two_dimensional_non_substitutability() {
    let (m, a) = (DnsReorgMode::TwoDimensionalDominance, DnsRolloutStage::Active);
    // beats BOTH → accepted
    assert_eq!(check_dns_reorg_rule(&reorg_inputs(a, m, false, 200, 100, 200, 100)), DnsReorgOutcome::DominanceSatisfied);
    // beats WORK only (stake equal) → rejected (non-substitutability)
    assert_eq!(check_dns_reorg_rule(&reorg_inputs(a, m, false, 200, 100, 100, 100)), DnsReorgOutcome::DominanceViolation);
    // beats STAKE only (work equal) → rejected
    assert_eq!(check_dns_reorg_rule(&reorg_inputs(a, m, false, 100, 100, 200, 100)), DnsReorgOutcome::DominanceViolation);
    // ties on both (must STRICTLY beat) → rejected
    assert_eq!(check_dns_reorg_rule(&reorg_inputs(a, m, false, 100, 100, 100, 100)), DnsReorgOutcome::DominanceViolation);
}

/// Partition-liveness override (incident 2026-08-03 §8). A stake-less candidate that
/// out-Works canonical since the common ancestor by MORE than the multiplier is accepted;
/// at or below it the strict non-substitutability rule still rejects. This is what stops a
/// partitioned minority branch from wedging forever against the work-dominant chain.
#[test]
fn dns_reorg_work_override_releases_stake_veto() {
    let (m, a) = (DnsReorgMode::TwoDimensionalDominance, DnsRolloutStage::Active);
    let with_mult = |cw: u64, kw: u64, mult: u32| {
        let mut i = reorg_inputs(a, m, false, cw, kw, 0, 500); // candidate stake 0, canonical 500
        i.emergency_work_override_multiplier = mult;
        check_dns_reorg_rule(&i)
    };
    // 5x > 4x → override releases the stake veto even though candidate stake is ZERO.
    assert_eq!(with_mult(500, 100, 4), DnsReorgOutcome::WorkDominanceOverride);
    assert!(with_mult(500, 100, 4).is_accept());
    // Exactly 4x is NOT "more than 4x" → still rejected (strict comparison).
    assert_eq!(with_mult(400, 100, 4), DnsReorgOutcome::DominanceViolation);
    // 3x → rejected: an ordinary fork must not release the veto.
    assert_eq!(with_mult(300, 100, 4), DnsReorgOutcome::DominanceViolation);
    // multiplier 0 disables the override entirely (strict legacy behaviour).
    assert_eq!(with_mult(10_000, 100, 0), DnsReorgOutcome::DominanceViolation);
    // The override is independent of `emergency_work_margin` — an absolute margin far above
    // the real work scale (the testnet-22 condition) must not disable it.
    let mut i = reorg_inputs(a, m, false, 500, 100, 0, 500);
    i.emergency_work_override_multiplier = 4;
    i.emergency_work_margin = BlueWorkType::from_u64(1_000_000); // unreachable at this scale
    assert_eq!(check_dns_reorg_rule(&i), DnsReorgOutcome::WorkDominanceOverride);
    // Overflow in the bound computation must fail CLOSED (no override), never open.
    let mut i = reorg_inputs(a, m, false, 1, 0, 0, 500);
    i.canonical_work_after = BlueWorkType::MAX;
    i.emergency_work_override_multiplier = 4;
    assert_eq!(check_dns_reorg_rule(&i), DnsReorgOutcome::DominanceViolation);
}

/// Regression for the testnet-22 deadlock (incident 2026-08-03 §8) using the reported field
/// values. Branch A (community) held blueWork 429,462 and branch B (validators) 217,934, split
/// at daa 114,307. Branch B had confirmed a branch-B-ONLY anchor, so branch A's candidates
/// exited the confirmed prefix and scored stake 0 against branch B's own bond view — branch B
/// could never accept branch A, and the partition could not self-heal. With the override the
/// minority branch rejoins.
#[test]
fn testnet22_partition_deadlock_is_released() {
    let (m, a) = (DnsReorgMode::TwoDimensionalDominance, DnsRolloutStage::Active);
    // Since-ancestor deltas at the split: branch A accumulated ~219k, branch B only ~8k.
    let (branch_a_after, branch_b_after) = (219_462u64, 7_934u64);
    // Branch B's view: candidate = branch A (invisible stake ⇒ 0), canonical = branch B.
    let mut i = reorg_inputs(a, m, false, branch_a_after, branch_b_after, 0, 12);
    i.emergency_work_margin = BlueWorkType::from_u64(1_000_000); // the real testnet-10/22 value
    i.emergency_stake_margin = StakeScore(100 * STAKE_SCORE_SCALE);

    // Before the fix (override disabled): permanent RejectedDominance — the observed deadlock.
    i.emergency_work_override_multiplier = 0;
    assert_eq!(
        check_dns_reorg_rule(&i),
        DnsReorgOutcome::DominanceViolation,
        "reproduces the incident: the work-dominant branch is refused forever"
    );

    // After the fix: branch A out-works branch B by ~27x ≫ 4x → the veto releases.
    i.emergency_work_override_multiplier = 4;
    assert_eq!(check_dns_reorg_rule(&i), DnsReorgOutcome::WorkDominanceOverride);
    assert!(check_dns_reorg_rule(&i).is_accept(), "the partitioned minority branch can rejoin");
}

/// Regression for the 2026-07-19 testnet-10 virtual freeze (§2-2/§2-3/§2-4). An isolated node
/// kept mining its own branch at ~45 s/block while the network advanced at 10 bps; the gate
/// refused every block of the network's chain for 3.5 h, silently, and a restart did not clear
/// it (the self-referential anchor was persisted). Both layers that made it permanent are
/// covered: an unsatisfiable stake margin, and the absence of any work-based release.
#[test]
fn testnet10_virtual_freeze_is_released() {
    let (m, a) = (DnsReorgMode::TwoDimensionalDominance, DnsRolloutStage::Active);
    // Over 3.5 h the network's chain accumulated ~126_000 blocks of work; the isolated
    // self-branch ~280. Candidate = the network's chain, whose stake is invisible from the
    // wedged node's own bond view and therefore scores 0.
    let mut i = reorg_inputs(a, m, false, 126_000, 280, 0, STAKE_SCORE_SCALE);
    i.emergency_work_margin = BlueWorkType::from_u64(1_000_000);

    // §2-4: the historical margin (1e11) sat ABOVE the 15-epoch window ceiling (1.5e10), so
    // `DominanceSatisfied` was unreachable for ANY chain — the escape valve did not exist.
    i.emergency_stake_margin = StakeScore(100 * STAKE_SCORE_SCALE);
    i.emergency_work_override_multiplier = 0;
    assert_eq!(
        check_dns_reorg_rule(&i),
        DnsReorgOutcome::DominanceViolation,
        "reproduces the freeze: the network's own chain is refused indefinitely"
    );

    // After the fix the calibrated margin and the work override both apply; the node rejoins.
    i.emergency_stake_margin = StakeScore(3 * STAKE_SCORE_SCALE);
    i.emergency_work_override_multiplier = 4;
    assert_eq!(check_dns_reorg_rule(&i), DnsReorgOutcome::WorkDominanceOverride);
    assert!(check_dns_reorg_rule(&i).is_accept(), "the wedged node can follow the network again");
}

/// incident 2026-07-19 §2-4. `StakeScore` is WINDOWED, so it has a hard ceiling of
/// `floor(window / epoch_length) × SCALE`. A margin at or above that ceiling is unsatisfiable
/// by construction, which silently turns `TwoDimensionalDominance` into `HardCheckpoint` —
/// exactly the failure that shipped. This test makes that class of miscalibration impossible
/// to reintroduce unnoticed.
#[test]
fn presets_emergency_stake_margin_is_attainable() {
    use crate::config::params::{GENESIS_ACTIVE_DNS_PARAMS, PRODUCTION_DNS_PARAMS, TESTNET_DNS_PARAMS};
    for (name, p) in [
        ("genesis-active/dev-sim", GENESIS_ACTIVE_DNS_PARAMS),
        ("production/mainnet", PRODUCTION_DNS_PARAMS),
        ("testnet", TESTNET_DNS_PARAMS),
    ] {
        let ceiling = p.max_attainable_stake_score();
        assert!(ceiling.0 > 0, "{name}: the StakeScore window must admit at least one epoch");
        assert!(
            p.emergency_stake_margin_is_attainable(),
            "{name}: emergency_stake_margin {} is >= the attainable StakeScore ceiling {} — the DominanceSatisfied escape path would be dead code",
            p.emergency_stake_margin.0,
            ceiling.0
        );
        // The historical value must NOT pass, or this test would not have caught the bug.
        let mut regressed = p.clone();
        regressed.emergency_stake_margin = StakeScore(100 * STAKE_SCORE_SCALE);
        assert!(
            !regressed.emergency_stake_margin_is_attainable(),
            "{name}: the shipped-and-broken 1e11 margin must be rejected by this check"
        );
    }
}

/// **Bond-stamp path-independence** (the overlay-commitment divergence class, incident
/// 2026-08-03 report §8 "946d7210 の overlay commitment 発散").
///
/// The bond state at a block must be a pure function of that block's chain — never of the
/// traversal path a node happened to take to reach it. `apply`/`revert` are the only mutators
/// of that state during a reorg walk, so they must be exact inverses. Before first-stamp-wins
/// they were not: reverting a branch's duplicate `Slash` cleared a slash the COMMON PREFIX had
/// stamped, resurrecting the bond as un-slashed on a node that had traversed the branch, while
/// a node that walked straight to the same block kept it slashed. Different bond sets for
/// byte-identical history ⇒ different `overlay_commitment_root` ⇒ mutual block rejection.
#[test]
fn bond_view_walk_is_path_independent_across_a_reorg() {
    let op = fixture_outpoint();
    let bond = stake_bond_record_from_payload(&fixture_bond(), op);
    let insert = vec![BondMutation::Insert(op, bond.clone())];
    // The bond activates at 5_000 (the fixture). The common prefix slashes it at DAA 6_000; a
    // branch block slashes it AGAIN at 6_500. Reachable: `slashing_evidence_genuine` judges the
    // bond's status at the ATTESTATION's target DAA, so a second genuine evidence for an
    // already-slashed bond is accepted.
    let prefix_slash = vec![BondMutation::Slash(op, 6_000)];
    let branch_slash = vec![BondMutation::Slash(op, 6_500)];

    // Path 1 — the direct walk (a node that never saw the branch).
    let mut direct = ActiveBondView::new();
    direct.apply(&insert);
    direct.apply(&prefix_slash);

    // Path 2 — walk onto the branch, then reorg off it (a node that did).
    let mut via_branch = ActiveBondView::new();
    via_branch.apply(&insert);
    via_branch.apply(&prefix_slash);
    via_branch.apply(&branch_slash);
    via_branch.revert(&branch_slash);

    assert_eq!(
        via_branch.get(&op).unwrap().slashed_at_daa_score,
        Some(6_000),
        "reverting the branch's duplicate slash must not clear the PREFIX's slash"
    );
    assert_eq!(via_branch, direct, "the bond view at a block must not depend on how the node got there");
    // The consequence that actually split the network: the two views commit to the same root.
    let snap = |v: &ActiveBondView| OverlaySnapshot { bonds: v.records(), reserve_balance: 0, window: Vec::new() }.commitment_root();
    assert_eq!(snap(&via_branch), snap(&direct), "same history ⇒ same overlay commitment");
    // And the bond is still Slashed at a POV past the prefix stamp (reward/attestation eligibility).
    assert_eq!(effective_bond_status(via_branch.get(&op).unwrap(), 6_100), BondStatus::Slashed);

    // Reachability of the scenario, in one assertion: `slashing_evidence_genuine` gates a
    // second evidence on the bond's status at the ATTESTATION's target DAA — not at the
    // including block — and a bond slashed at 6_000 was still Active at 5_500. So a second
    // genuine evidence (targets at 5_500, included at 6_001, inside `evidence_window_blocks`)
    // is accepted on a chain where the bond is already slashed, which is what produces the
    // duplicate `Slash` mutation this test reverts. The bug was live, not merely theoretical.
    assert_eq!(effective_bond_status(direct.get(&op).unwrap(), 5_500), BondStatus::Active);
}

/// `apply` then `revert` of the SAME mutation list is the identity **over the canonical timing
/// fields** — the ones [`effective_bond_status`] reads and the overlay commitment therefore
/// depends on. Covers every mutation kind and duplicates inside one block (two evidences
/// accepted by one chain block share its DAA score, so `revert_bond_stamp`'s equality test sees
/// the same value twice — reverse order still lands on the correct end state).
///
/// The raw `status` field is deliberately excluded: it is vestigial (no consensus rule reads
/// it; every consumer goes through `effective_bond_status`, and `compute_overlay_snapshot`
/// normalizes it before hashing), and `revert` cannot restore a `Pending`-before-slash bond to
/// `Pending` without an undo log. Normalizing is what makes a never-slashed and a
/// slashed-then-reverted bond commit to the same root.
#[test]
fn bond_stamp_apply_revert_round_trips() {
    let op = fixture_outpoint();
    let bond = stake_bond_record_from_payload(&fixture_bond(), op);
    let seeded = || {
        let mut v = ActiveBondView::new();
        v.apply(&[BondMutation::Insert(op, bond.clone())]);
        v
    };
    // Compare as the commitment does: raw `status` normalized to the effective one.
    let normalized = |v: &ActiveBondView| {
        let mut recs = v.records();
        for r in recs.iter_mut() {
            r.status = effective_bond_status(r, 10_000);
        }
        recs.sort_by_key(|r| (r.bond_outpoint.transaction_id, r.bond_outpoint.index));
        recs
    };
    for muts in [
        vec![BondMutation::Slash(op, 900)],
        vec![BondMutation::Unbond(op, 900, None)],
        // Two stamps of each kind inside ONE block (same DAA), plus a mixed block.
        vec![BondMutation::Slash(op, 900), BondMutation::Slash(op, 900)],
        vec![BondMutation::Unbond(op, 900, None), BondMutation::Unbond(op, 900, None)],
        vec![BondMutation::Unbond(op, 900, None), BondMutation::Slash(op, 900)],
    ] {
        let mut v = seeded();
        v.apply(&muts);
        v.revert(&muts);
        assert_eq!(normalized(&v), normalized(&seeded()), "apply∘revert must be the identity for {muts:?}");
        let r = v.get(&op).unwrap();
        assert_eq!((r.slashed_at_daa_score, r.unbond_request_daa_score), (None, None), "both stamps cleared for {muts:?}");
    }
    // An `Insert` reverted in the same diff as the stamps that followed it removes the bond
    // entirely (reverse order handles the ordering).
    let muts = vec![BondMutation::Insert(op, bond.clone()), BondMutation::Slash(op, 900)];
    let mut v = ActiveBondView::new();
    v.apply(&muts);
    v.revert(&muts);
    assert!(v.is_empty(), "reverting the insert removes the bond");
}

/// First stamp wins: a later duplicate does not move the recorded DAA, so
/// `effective_bond_status` keeps reporting the bond slashed from the EARLIER score. This is
/// also what makes the value well-defined on a linear chain (it used to be last-write-wins,
/// i.e. dependent on which duplicate a node saw last).
#[test]
fn bond_stamp_first_write_wins() {
    let op = fixture_outpoint();
    let mut rec = stake_bond_record_from_payload(&fixture_bond(), op);
    apply_bond_stamp(&mut rec, &BondMutation::Slash(op, 100));
    apply_bond_stamp(&mut rec, &BondMutation::Slash(op, 150));
    assert_eq!(rec.slashed_at_daa_score, Some(100));
    // Reverting the LATER stamp is a no-op; only the matching one clears.
    revert_bond_stamp(&mut rec, &BondMutation::Slash(op, 150));
    assert_eq!(rec.slashed_at_daa_score, Some(100));
    revert_bond_stamp(&mut rec, &BondMutation::Slash(op, 100));
    assert_eq!(rec.slashed_at_daa_score, None);
    // Same for the unbond clock.
    let mut rec = stake_bond_record_from_payload(&fixture_bond(), op);
    apply_bond_stamp(&mut rec, &BondMutation::Unbond(op, 100, None));
    apply_bond_stamp(&mut rec, &BondMutation::Unbond(op, 150, None));
    assert_eq!(rec.unbond_request_daa_score, Some(100));
    revert_bond_stamp(&mut rec, &BondMutation::Unbond(op, 150, None));
    assert_eq!(rec.unbond_request_daa_score, Some(100));
}

/// Confirmed-anchor TTL (`dns_veto_ttl_daa_score`): an anchor whose branch stopped confirming
/// stops being a veto once the node's OWN chain has advanced past the TTL — the testnet-20
/// dead-branch wedge shape, where a node keeps producing blocks on a branch no validator
/// attests to and the previously-confirmed anchor is carried forward forever.
#[test]
fn dns_reorg_stale_anchor_releases_the_veto() {
    let (m, a) = (DnsReorgMode::TwoDimensionalDominance, DnsRolloutStage::Active);
    // A candidate with NO work and NO stake advantage — refused on every dimension while the
    // anchor is fresh, so only staleness can explain a release here.
    let with_age = |age: u64, ttl: u64| {
        let mut i = reorg_inputs(a, m, false, 100, 100, 0, 500);
        i.emergency_work_override_multiplier = 4;
        i.confirmed_anchor_age_daa_score = age;
        i.veto_ttl_daa_score = ttl;
        check_dns_reorg_rule(&i)
    };
    // Fresh anchor (the healthy lag+epoch distance) ⇒ the veto stands.
    assert_eq!(with_age(200, 6_000), DnsReorgOutcome::DominanceViolation);
    // Exactly at the TTL is NOT past it (strict comparison, like every other threshold here).
    assert_eq!(with_age(6_000, 6_000), DnsReorgOutcome::DominanceViolation);
    // One DAA past ⇒ released.
    assert_eq!(with_age(6_001, 6_000), DnsReorgOutcome::ConfirmedAnchorStale);
    assert!(with_age(6_001, 6_000).is_accept());
    assert!(with_age(6_001, 6_000).is_liveness_release());
    // TTL disabled ⇒ the pre-TTL behaviour at ANY age (the veto never expires).
    assert_eq!(with_age(u64::MAX, u64::MAX), DnsReorgOutcome::DominanceViolation);
    // The TTL never overrides "the candidate keeps the confirmed anchor" — that path accepts
    // first and is not a release, so the distinction stays visible in the logs.
    let mut keeps = reorg_inputs(a, m, true, 100, 100, 0, 500);
    keeps.confirmed_anchor_age_daa_score = 99_999;
    keeps.veto_ttl_daa_score = 10;
    assert_eq!(check_dns_reorg_rule(&keeps), DnsReorgOutcome::IncludesConfirmedAnchor);
    // Nor does it wake a dormant gate.
    let mut dormant = reorg_inputs(DnsRolloutStage::Bootstrap, m, false, 100, 100, 0, 500);
    dormant.confirmed_anchor_age_daa_score = 99_999;
    dormant.veto_ttl_daa_score = 10;
    assert_eq!(check_dns_reorg_rule(&dormant), DnsReorgOutcome::GateInactive);
    // It releases `HardCheckpoint` too — the same unreleasable-veto shape lives there.
    let mut hc = reorg_inputs(a, DnsReorgMode::HardCheckpoint, false, 1, 100, 0, 500);
    hc.confirmed_anchor_age_daa_score = 11;
    hc.veto_ttl_daa_score = 10;
    assert_eq!(check_dns_reorg_rule(&hc), DnsReorgOutcome::ConfirmedAnchorStale);
}

/// The TTL clock is the node's OWN canonical tip, so a candidate branch cannot buy its way
/// past the veto by being far ahead in DAA. This is the property that keeps the TTL from
/// becoming a stake-substitution lever: an attacker mining a secret branch to an arbitrary
/// height changes nothing about whether the victim's own anchor has aged out.
#[test]
fn stale_anchor_clock_ignores_the_candidate_branch() {
    use crate::config::params::PRODUCTION_DNS_PARAMS;
    let p = DnsParams { dns_veto_ttl_daa_score: 6_000, ..PRODUCTION_DNS_PARAMS.clone() };
    // Victim's own chain sits 200 DAA past its anchor (healthy) — the attacker's branch being
    // 10_000_000 DAA ahead is not an input to this predicate at all.
    assert!(!p.confirmed_anchor_is_stale(1_000_200, 1_000_000), "a healthy own-chain distance is never stale");
    // The victim's own chain going quiet on confirmations IS what ages the anchor.
    assert!(p.confirmed_anchor_is_stale(1_006_001, 1_000_000));
    // Saturating: an anchor ahead of the tip (a fresh confirmation racing the read) is fresh.
    assert!(!p.confirmed_anchor_is_stale(1_000, 900_000));
    // u64::MAX TTL is "never stale" even at the extreme.
    let never = DnsParams { dns_veto_ttl_daa_score: u64::MAX, ..PRODUCTION_DNS_PARAMS.clone() };
    assert!(!never.confirmed_anchor_is_stale(u64::MAX, 0));
}

/// The gate's reach must be a deliberate number. `max_reorg_horizon_blocks` is 300 blocks —
/// **30 seconds at 10 BPS** — and since the gate abstains past the horizon, leaving the two
/// tied would mean DNS finality protects a 30-second window and nothing older. The production
/// presets must therefore carry an explicit, larger gate horizon, and a TTL with real headroom
/// over the healthy anchor-to-tip distance (`lag + epoch`), or confirmations would expire as
/// fast as they are made.
#[test]
fn presets_veto_reach_is_calibrated() {
    use crate::config::params::{GENESIS_ACTIVE_DNS_PARAMS, PRODUCTION_DNS_PARAMS, TESTNET_DNS_PARAMS};
    for (name, p) in [("production/mainnet", PRODUCTION_DNS_PARAMS), ("testnet", TESTNET_DNS_PARAMS)] {
        assert!(
            p.veto_reach_is_coherent(),
            "{name}: gate horizon {} / TTL {} is incoherent against reorg horizon {} and lag+epoch {}",
            p.gate_horizon_blocks(),
            p.dns_veto_ttl_daa_score,
            p.max_reorg_horizon_blocks,
            p.attestation_lag_blue_score + p.attestation_epoch_length_blue_score,
        );
        assert!(
            p.gate_horizon_blocks() > p.max_reorg_horizon_blocks,
            "{name}: the gate must reach FURTHER than the 300-block economic horizon (30 s at 10 BPS), else DNS finality is a 30-second property"
        );
        assert!(
            p.dns_veto_ttl_daa_score < p.gate_horizon_blocks(),
            "{name}: a TTL at or beyond the horizon can never fire — the horizon abstain would always win"
        );
        assert_ne!(p.dns_veto_ttl_daa_score, u64::MAX, "{name}: an unreleasable veto is what made partitions permanent");
    }
    // devnet/simnet keep the tied-to-`max_reorg_horizon_blocks` behaviour on purpose: the DAG
    // simulation fixtures tune that one knob and expect the gate to follow it.
    assert_eq!(GENESIS_ACTIVE_DNS_PARAMS.dns_gate_horizon_blocks, 0);
    assert_eq!(GENESIS_ACTIVE_DNS_PARAMS.gate_horizon_blocks(), GENESIS_ACTIVE_DNS_PARAMS.max_reorg_horizon_blocks);
    // A veto arm-able by ONE signer is only acceptable where the active set is deliberately one
    // operator. Mainnet's floor is 3 validators, so it must require more than a single attester.
    assert_eq!(TESTNET_DNS_PARAMS.min_active_validators, 1);
    assert_eq!(TESTNET_DNS_PARAMS.min_anchor_attesters, 1, "single-operator mesh: 2 would mean nothing ever confirms");
    assert!(
        PRODUCTION_DNS_PARAMS.min_anchor_attesters > 1
            && PRODUCTION_DNS_PARAMS.min_anchor_attesters <= PRODUCTION_DNS_PARAMS.min_active_validators,
        "mainnet must need >1 distinct attester, but no more than its own validator floor"
    );
}

/// The override is a liveness floor for the whole gate, so it releases `HardCheckpoint` too —
/// that mode has the same unreleasable-veto shape and would wedge identically.
#[test]
fn dns_reorg_work_override_applies_to_hard_checkpoint() {
    let (m, a) = (DnsReorgMode::HardCheckpoint, DnsRolloutStage::Active);
    let mut i = reorg_inputs(a, m, false, 500, 100, 0, 0);
    i.emergency_work_override_multiplier = 4;
    assert_eq!(check_dns_reorg_rule(&i), DnsReorgOutcome::WorkDominanceOverride);
    i.emergency_work_override_multiplier = 0;
    assert_eq!(check_dns_reorg_rule(&i), DnsReorgOutcome::HardCheckpointReject);
}

/// Every shipped preset must carry a non-zero override, since the deadlock is reachable on any
/// network running `TwoDimensionalDominance` — which is all of them.
#[test]
fn all_presets_have_a_partition_liveness_override() {
    use crate::config::params::{GENESIS_ACTIVE_DNS_PARAMS, PRODUCTION_DNS_PARAMS, TESTNET_DNS_PARAMS};
    for (name, p) in [
        ("genesis-active/dev-sim", GENESIS_ACTIVE_DNS_PARAMS),
        ("production/mainnet", PRODUCTION_DNS_PARAMS),
        ("testnet", TESTNET_DNS_PARAMS),
    ] {
        assert_ne!(
            p.emergency_work_override_multiplier, 0,
            "{name} must keep a partition-liveness escape hatch: a stake veto with no release makes a network split permanent"
        );
        assert_eq!(p.reorg_mode, DnsReorgMode::TwoDimensionalDominance, "{name} runs the 2-D gate (so it is exposed)");
    }
}

/// Every guard in [`stake_preference_verdict`] exists for a named failure mode; this pins
/// each boundary from both sides so none of them can be quietly widened.
#[test]
fn stake_preference_is_an_escape_from_a_dead_branch_only() {
    let base = StakePreferenceInputs {
        rollout_stage: DnsRolloutStage::Active,
        own_anchor_age_daa_score: 6_001, // one past the TTL: own DNS is DEAD
        veto_ttl_daa_score: 6_000,
        multiplier: 2,
        candidate_work_after: BlueWorkType::from_u64(600),
        canonical_work_after: BlueWorkType::from_u64(1_000),
        candidate_stake_after: StakeScore(20 * STAKE_SCORE_SCALE),
        canonical_stake_after: StakeScore(0),
        emergency_stake_margin: StakeScore(3 * STAKE_SCORE_SCALE),
        required_stake_depth: StakeScore(10 * STAKE_SCORE_SCALE),
    };
    assert!(stake_preference_verdict(&base), "the qualifying case must qualify, or nothing below means anything");

    // Symmetric-live safety: a fresh own anchor disarms the preference entirely — validators
    // on BOTH sides of a partition means neither side gets pulled.
    assert!(!stake_preference_verdict(&StakePreferenceInputs { own_anchor_age_daa_score: 6_000, ..base.clone() }));
    assert!(!stake_preference_verdict(&StakePreferenceInputs { own_anchor_age_daa_score: 0, ..base.clone() }));

    // Not Active, or disabled: inert.
    assert!(!stake_preference_verdict(&StakePreferenceInputs { rollout_stage: DnsRolloutStage::Bootstrap, ..base.clone() }));
    assert!(!stake_preference_verdict(&StakePreferenceInputs { multiplier: 0, ..base.clone() }));

    // Hysteresis ON-bar: below confirmation-grade absolute depth, tentative DNS moves nothing —
    // even with total relative dominance.
    assert!(!stake_preference_verdict(&StakePreferenceInputs {
        candidate_stake_after: StakeScore(10 * STAKE_SCORE_SCALE - 1),
        ..base.clone()
    }));
    assert!(!stake_preference_verdict(&StakePreferenceInputs { candidate_stake_after: StakeScore(0), ..base.clone() }));

    // Relative dominance must clear the margin strictly.
    assert!(!stake_preference_verdict(&StakePreferenceInputs {
        candidate_stake_after: StakeScore(13 * STAKE_SCORE_SCALE),
        canonical_stake_after: StakeScore(10 * STAKE_SCORE_SCALE),
        ..base.clone()
    }));
    assert!(stake_preference_verdict(&StakePreferenceInputs {
        candidate_stake_after: StakeScore(13 * STAKE_SCORE_SCALE + 1),
        canonical_stake_after: StakeScore(10 * STAKE_SCORE_SCALE),
        ..base.clone()
    }));

    // The deficit bound: at multiplier 2, a branch with exactly half the work fails (500×2 is
    // not > 1000) and one just above half passes — and a >4× work advantage on the canonical
    // side (the emergency-override regime) is therefore unreachable from this rule.
    assert!(!stake_preference_verdict(&StakePreferenceInputs { candidate_work_after: BlueWorkType::from_u64(500), ..base.clone() }));
    assert!(stake_preference_verdict(&StakePreferenceInputs { candidate_work_after: BlueWorkType::from_u64(501), ..base.clone() }));
    assert!(!stake_preference_verdict(&StakePreferenceInputs { candidate_work_after: BlueWorkType::from_u64(249), ..base.clone() }));
}

/// Mainnet ships the preference OFF until the partition/IBD scenario matrix has run; test
/// networks ship it ON so the soak and the regression harness actually exercise it.
#[test]
fn presets_declare_the_stake_preference_posture() {
    use crate::config::params::{GENESIS_ACTIVE_DNS_PARAMS, PRODUCTION_DNS_PARAMS, TESTNET_DNS_PARAMS};
    assert_eq!(
        PRODUCTION_DNS_PARAMS.stake_preference_max_work_deficit_multiplier, 0,
        "mainnet enables the stake preference only after the scenario matrix passes"
    );
    for (name, p) in [("genesis-active/dev-sim", GENESIS_ACTIVE_DNS_PARAMS), ("testnet", TESTNET_DNS_PARAMS)] {
        assert_eq!(p.stake_preference_max_work_deficit_multiplier, 2, "{name} soaks the preference at the ½-work bound");
    }
}

/// Settlement posture: testnet soaked the policy layer first and mainnet now takes it (the user's
/// Mainnet Decision A, made mainnet's on 2026-09-25); the dev/sim fixture presets ship it off.
/// The value must clear the dispute-resolution horizon — `gate_horizon + veto TTL` — or the
/// fallback goes liquid while a fork can still flip: on testnet, and on mainnet at the carded
/// cadence it launches at (on the bundle-free 10 bps preset 600 sits under the 1,000-block base
/// maturity, which `coinbase_spend_settled` never undercuts, so the knob is inert there).
#[test]
fn presets_declare_the_coinbase_settlement_posture() {
    use crate::config::params::{GENESIS_ACTIVE_DNS_PARAMS, MAINNET_PARAMS, PRODUCTION_DNS_PARAMS, TESTNET_DNS_PARAMS};
    let m = PRODUCTION_DNS_PARAMS.coinbase_settlement_long_maturity_daa;
    assert_eq!(m, 600, "mainnet: Decision A's DAA fallback, testnet's soaked value");
    let carded = PRODUCTION_DNS_PARAMS.at_two_minute_cadence();
    assert!(
        m > carded.gate_horizon_blocks() + carded.dns_veto_ttl_daa_score,
        "a carded mainnet's fallback outlasts its dispute horizon"
    );
    assert!(m <= MAINNET_PARAMS.coinbase_maturity(), "…and the bundle-free 10 bps preset's base maturity already covers it");
    assert_eq!(GENESIS_ACTIVE_DNS_PARAMS.coinbase_settlement_long_maturity_daa, 0, "dev/sim fixtures keep their semantics");
    let t = TESTNET_DNS_PARAMS.coinbase_settlement_long_maturity_daa;
    assert!(t > 0, "testnet soaks the settlement policy");
    for (name, p) in
        [("genesis-active", GENESIS_ACTIVE_DNS_PARAMS), ("production", PRODUCTION_DNS_PARAMS), ("testnet", TESTNET_DNS_PARAMS)]
    {
        assert_eq!(
            p.coinbase_settlement_consensus_activation_daa_score,
            u64::MAX,
            "{name}: consensus settlement must stay fenced until the anchor fold lands with its scenario matrix"
        );
    }
    assert!(
        t > TESTNET_DNS_PARAMS.gate_horizon_blocks() + TESTNET_DNS_PARAMS.dns_veto_ttl_daa_score,
        "the fallback must outlast the dispute-resolution horizon (got {t})"
    );
}

/// Every branch of [`coinbase_spend_settled`] pinned from both sides: the base floor is
/// unconditional, `None` settlement is byte-identical to the base rule, the long fallback
/// releases exactly at its bound, and acceleration requires the anchor AT or PAST the block.
#[test]
fn coinbase_settlement_accelerates_and_never_undercuts() {
    let s = |anchor: Option<u64>| DnsCoinbaseSettlement { long_maturity_daa: 30_000, confirmed_anchor_daa: anchor };

    // Base floor unconditional — even with a passed anchor.
    assert!(!coinbase_spend_settled(1_000, 1_099, 100, Some(&s(Some(2_000)))));
    assert!(coinbase_spend_settled(1_000, 1_100, 100, Some(&s(Some(2_000)))));

    // Feature off (None): base maturity alone — today's behaviour.
    assert!(coinbase_spend_settled(1_000, 1_100, 100, None));

    // No anchor: only the long fallback releases, exactly at its bound.
    assert!(!coinbase_spend_settled(1_000, 30_999, 100, Some(&s(None))));
    assert!(coinbase_spend_settled(1_000, 31_000, 100, Some(&s(None))));

    // Anchor short of the block grants nothing; at the block it grants settlement.
    assert!(!coinbase_spend_settled(1_000, 20_000, 100, Some(&s(Some(999)))));
    assert!(coinbase_spend_settled(1_000, 20_000, 100, Some(&s(Some(1_000)))));

    // A pov below the block (malformed context) never settles.
    assert!(!coinbase_spend_settled(1_000, 999, 0, Some(&s(Some(2_000)))));
}

#[test]
fn dns_reorg_dominance_respects_margins() {
    let (m, a) = (DnsReorgMode::TwoDimensionalDominance, DnsRolloutStage::Active);
    let mut i = reorg_inputs(a, m, false, 150, 100, 150, 100);
    i.emergency_work_margin = BlueWorkType::from_u64(60); // need cand_W > 160; 150 fails
    i.emergency_stake_margin = StakeScore(10);
    assert_eq!(check_dns_reorg_rule(&i), DnsReorgOutcome::DominanceViolation);
    i.candidate_work_after = BlueWorkType::from_u64(161); // clears work margin; stake 150 > 110 ok
    assert_eq!(check_dns_reorg_rule(&i), DnsReorgOutcome::DominanceSatisfied);
}

#[test]
fn reorg_inputs_since_common_ancestor_subtracts_work() {
    let w = BlueWorkType::from_u64;
    // work_after = blue_work(tip) − blue_work(common ancestor); cumulative → exact subtraction.
    // Stake is windowed (not cumulative) so it is passed through verbatim, not subtracted.
    let i = reorg_inputs_since_common_ancestor(
        DnsRolloutStage::Active,
        DnsReorgMode::TwoDimensionalDominance,
        false,
        w(1000), // candidate tip cumulative work
        w(900),  // canonical tip cumulative work
        w(500),  // common ancestor cumulative work
        StakeScore(7),
        StakeScore(3),
        w(0),
        StakeScore(0),
        0,
        0,        // fresh anchor
        u64::MAX, // TTL disabled: this test covers the since-ancestor derivation only
    );
    assert_eq!(i.candidate_work_after, w(500)); // 1000 − 500
    assert_eq!(i.canonical_work_after, w(400)); // 900 − 500
    assert_eq!((i.candidate_stake_after, i.canonical_stake_after), (StakeScore(7), StakeScore(3))); // passed through
    // Candidate out-works (500 > 400) AND out-stakes (7 > 3) → dominance satisfied.
    assert_eq!(check_dns_reorg_rule(&i), DnsReorgOutcome::DominanceSatisfied);

    // A tip that IS the common ancestor contributes zero since-ancestor work (saturating
    // floor) → it cannot dominate, so the gate rejects.
    let i = reorg_inputs_since_common_ancestor(
        DnsRolloutStage::Active,
        DnsReorgMode::TwoDimensionalDominance,
        false,
        w(500), // candidate tip == common ancestor
        w(900),
        w(500),
        StakeScore(9),
        StakeScore(1),
        w(0),
        StakeScore(0),
        0,
        0,        // fresh anchor
        u64::MAX, // TTL disabled: this test covers the since-ancestor derivation only
    );
    assert_eq!(i.candidate_work_after, w(0)); // 500 − 500, floored
    assert_eq!(i.canonical_work_after, w(400));
    assert_eq!(check_dns_reorg_rule(&i), DnsReorgOutcome::DominanceViolation); // zero work cannot dominate
}

#[test]
fn dns_constants_have_expected_values() {
    // Cross-check against the consensus-core kaspa-pq constant
    // values (the kaspa-txscript crate is downstream of
    // consensus-core, so we cannot pull MLDSA87_PK_LEN /
    // MLDSA87_SIG_LEN from there directly without creating a
    // dependency cycle; the values are duplicated here and the
    // assertion is the contract).
    assert_eq!(STAKE_VALIDATOR_PUBKEY_LEN, 2592);
    assert_eq!(STAKE_ATTESTATION_SIG_LEN, 4627);

    // ADR-0009 / ADR-0010 / ADR-0012 / ADR-0014 domain-
    // separator strings. All consensus-fixed (or consensus-
    // adjacent, for the node-local failover keys) and bumped
    // only by a hard-fork ADR — pin the bytes so any
    // accidental rename trips this test.
    assert_eq!(ATTESTATION_MLDSA87_CONTEXT, b"kaspa-pq-v1/att/mldsa87");
    assert_eq!(ATTESTATION_MESSAGE_DOMAIN, b"kaspa-pq-v1/stake-attestation");
    assert_eq!(VALIDATOR_SET_COMMITMENT_KEY, b"kaspa-pq-validator-set-v1");
    // ADR-0014 — node-local failover protocol keys.
    assert_eq!(HOST_ID_KEY, b"kaspa-pq-validator-host-id-v1");
    assert_eq!(TAKEOVER_TOKEN_MESSAGE_DOMAIN, b"kaspa-pq-takeover-token-v1");
    assert_eq!(TAKEOVER_TOKEN_CONTEXT, b"kaspa-pq-v1/takeover/mldsa87");
    // ADR-0015 — node-local remote-signer audit chain key
    // and protocol-version pin.
    assert_eq!(AUDIT_LOG_CHAIN_KEY, b"kaspa-pq-signer-audit-v1");
    // ADR-0015 / audit M-04 — audit-checkpoint signing context.
    assert_eq!(AUDIT_CHECKPOINT_MLDSA87_CONTEXT, b"kaspa-pq-v1/audit-ckpt/mldsa87");
    assert_eq!(SIGNER_PROTOCOL_VERSION, 1);
    // ADR-0015 capability bitflags must be single-bit and
    // pairwise distinct so they compose correctly under
    // bitwise OR.
    let caps = [CAP_SIGN_TRANSACTION, CAP_SIGN_ATTESTATION, CAP_SIGN_TAKEOVER_TOKEN, CAP_POLICY_STRICT, CAP_AUDIT_LOG, CAP_HSM_BACKED];
    for c in caps {
        assert!(c.count_ones() == 1, "capability {c:#x} is not a single bit");
    }
    for i in 0..caps.len() {
        for j in (i + 1)..caps.len() {
            assert_eq!(caps[i] & caps[j], 0, "capabilities {i} and {j} overlap");
        }
    }

    // Replay safety: tx vs attestation vs takeover contexts
    // must all differ (ADR-0002 / ADR-0009 §"Attestation
    // target" / ADR-0014 §"Public-claim discipline").
    assert_ne!(ATTESTATION_MLDSA87_CONTEXT, b"kaspa-pq-v1/tx/mldsa87");
    assert_ne!(TAKEOVER_TOKEN_CONTEXT, b"kaspa-pq-v1/tx/mldsa87");
    assert_ne!(TAKEOVER_TOKEN_CONTEXT, ATTESTATION_MLDSA87_CONTEXT);
    // audit M-04 — a checkpoint signature must not collide with
    // any overlay/tx domain, else it could be replayed as one.
    assert_ne!(AUDIT_CHECKPOINT_MLDSA87_CONTEXT, b"kaspa-pq-v1/tx/mldsa87");
    assert_ne!(AUDIT_CHECKPOINT_MLDSA87_CONTEXT, ATTESTATION_MLDSA87_CONTEXT);
    assert_ne!(AUDIT_CHECKPOINT_MLDSA87_CONTEXT, UNBOND_REQUEST_CONTEXT);
    assert_ne!(AUDIT_CHECKPOINT_MLDSA87_CONTEXT, TAKEOVER_TOKEN_CONTEXT);
}

#[test]
fn stake_score_display() {
    assert_eq!(StakeScore(0).to_string(), "0.000000000");
    assert_eq!(StakeScore(STAKE_SCORE_SCALE).to_string(), "1.000000000");
    assert_eq!(StakeScore(STAKE_SCORE_SCALE + 500_000_000).to_string(), "1.500000000");
    assert_eq!(StakeScore(STAKE_SCORE_SCALE * 3 / 4).to_string(), "0.750000000");
}

fn fixture_outpoint() -> TransactionOutpoint {
    // PR-9.5c: `TransactionOutpoint.transaction_id` widened to
    // `TransactionId` (= Hash64).
    TransactionOutpoint::new(Hash64::from_bytes([0x77u8; 64]), 42)
}

/// ADR-0018 §F canonical splits (Node share = 0): Stage-3 subsidy 67/8/25/0,
/// normal-tx 90/10/0, finality 75/25/0 (each group sums to 100%).
fn fixture_fee_split() -> FeeSplitParams {
    FeeSplitParams {
        subsidy_worker_base_bps: 6700,
        subsidy_worker_inclusion_bps: 800,
        subsidy_validator_bps: 2500,
        subsidy_service_bps: 0,
        normal_fee_worker_bps: 9000,
        normal_fee_validator_bps: 1000,
        normal_fee_service_bps: 0,
        finality_fee_validator_bps: 7500,
        finality_fee_worker_bps: 2500,
        finality_fee_service_bps: 0,
    }
}

/// ADR-0018 §F Stage-2 bootstrap split (smaller validator share): subsidy
/// 82/8/10/0, normal-tx 90/10/0, finality 75/25/0.
fn fixture_fee_split_bootstrap() -> FeeSplitParams {
    FeeSplitParams {
        subsidy_worker_base_bps: 8200,
        subsidy_worker_inclusion_bps: 800,
        subsidy_validator_bps: 1000,
        subsidy_service_bps: 0,
        normal_fee_worker_bps: 9000,
        normal_fee_validator_bps: 1000,
        normal_fee_service_bps: 0,
        finality_fee_validator_bps: 7500,
        finality_fee_worker_bps: 2500,
        finality_fee_service_bps: 0,
    }
}

fn fixture_attestation() -> StakeAttestation {
    StakeAttestation {
        version: DNS_PAYLOAD_VERSION_V1,
        validator_id: Hash64::from_bytes([0xa5u8; 64]),
        bond_outpoint: fixture_outpoint(),
        epoch: 7,
        target_hash: Hash64::from_bytes([0x11u8; 64]),
        target_daa_score: 1_234_567,
        validator_set_commitment: Hash64::default(), // audit #4: VSC is a fixed-zero invariant
        signature: vec![0x33u8; STAKE_ATTESTATION_SIG_LEN],
    }
}

fn fixture_validators() -> Vec<ValidatorRecord> {
    vec![
        ValidatorRecord { validator_id: Hash64::from_bytes([0xcc; 64]), stake_amount: 30, activation_daa_score: 300 },
        ValidatorRecord { validator_id: Hash64::from_bytes([0xaa; 64]), stake_amount: 10, activation_daa_score: 100 },
        ValidatorRecord { validator_id: Hash64::from_bytes([0xbb; 64]), stake_amount: 20, activation_daa_score: 200 },
    ]
}

#[test]
fn stake_bond_payload_borsh_roundtrip() {
    let bond = StakeBondPayload {
        version: DNS_PAYLOAD_VERSION_V1,
        owner_pubkey_hash: Hash64::from_bytes([0xaau8; 64]),
        validator_pubkey_hash: Hash64::from_bytes([0xbbu8; 64]),
        validator_pubkey: vec![0xccu8; STAKE_VALIDATOR_PUBKEY_LEN],
        amount: 100_000_000_000,
        activation_daa_score: 5_000,
        unbonding_period_blocks: 100_000,
        owner_reward_spk_payload: [0xddu8; 64],
    };
    let bytes = borsh::to_vec(&bond).unwrap();
    let back: StakeBondPayload = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, bond);
}

#[test]
fn stake_attestation_borsh_roundtrip() {
    let att = fixture_attestation();
    let bytes = borsh::to_vec(&att).unwrap();
    let back: StakeAttestation = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, att);
    // Spot-check the dominant size component: the ML-DSA-87
    // signature plus borsh framing. The Vec<u8> Borsh layout is
    // 4-byte length prefix + N data bytes, so a 4627-byte sig
    // contributes 4 + 4627 = 4631 bytes plus the other fixed
    // fields.
    assert!(bytes.len() >= STAKE_ATTESTATION_SIG_LEN);
}

#[test]
fn stake_attestation_shard_borsh_roundtrip() {
    let shard = StakeAttestationShardPayload {
        version: DNS_PAYLOAD_VERSION_V1,
        epoch: 7,
        target_hash: Hash64::from_bytes([0x11u8; 64]),
        target_daa_score: 1_234_567,
        validator_set_commitment: Hash64::from_bytes([0x22u8; 64]),
        attestations: vec![fixture_attestation(); 8],
    };
    assert!(shard.attestations.len() <= MAX_ATTESTATIONS_PER_SHARD);
    let bytes = borsh::to_vec(&shard).unwrap();
    let back: StakeAttestationShardPayload = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, shard);
}

#[test]
fn slashing_evidence_borsh_roundtrip() {
    let evidence = SlashingEvidencePayload {
        version: DNS_PAYLOAD_VERSION_V1,
        bond_outpoint: fixture_outpoint(),
        attestation_a: fixture_attestation(),
        attestation_b: {
            let mut b = fixture_attestation();
            b.target_hash = Hash64::from_bytes([0x33u8; 64]);
            b
        },
        reporter_reward_spk_payload: [0xeeu8; 64],
    };
    let bytes = borsh::to_vec(&evidence).unwrap();
    let back: SlashingEvidencePayload = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, evidence);
}

// ---- PR-10.4: DNS overlay tx kinds + stateless payload validation ----

fn fixture_bond() -> StakeBondPayload {
    let validator_pubkey = vec![0xccu8; STAKE_VALIDATOR_PUBKEY_LEN];
    StakeBondPayload {
        version: DNS_PAYLOAD_VERSION_V1,
        owner_pubkey_hash: Hash64::from_bytes([0xaau8; 64]),
        // audit H-04: declare the canonical key-derived overlay identity.
        validator_pubkey_hash: validator_id_from_pubkey(&validator_pubkey),
        validator_pubkey,
        amount: 100_000_000_000,
        activation_daa_score: 5_000,
        unbonding_period_blocks: 100_000,
        owner_reward_spk_payload: [0xddu8; 64],
    }
}

fn fixture_shard(n: usize) -> StakeAttestationShardPayload {
    StakeAttestationShardPayload {
        version: DNS_PAYLOAD_VERSION_V1,
        epoch: 7,
        target_hash: Hash64::from_bytes([0x11u8; 64]),
        target_daa_score: 1_234_567,
        validator_set_commitment: Hash64::default(), // audit #4: VSC is a fixed-zero invariant
        attestations: vec![fixture_attestation(); n],
    }
}

fn fixture_evidence() -> SlashingEvidencePayload {
    SlashingEvidencePayload {
        version: DNS_PAYLOAD_VERSION_V1,
        bond_outpoint: fixture_outpoint(),
        attestation_a: fixture_attestation(),
        attestation_b: {
            let mut b = fixture_attestation();
            b.target_hash = Hash64::from_bytes([0x33u8; 64]); // different anchor → equivocation
            b
        },
        reporter_reward_spk_payload: [0xeeu8; 64],
    }
}

#[test]
fn single_attestation_shard_tx_builds_a_valid_overlay_tx() {
    let att = fixture_attestation();
    let shard = single_attestation_shard(att.clone());
    assert_eq!(shard.attestations, vec![att.clone()]);
    // Shard-level tuple is copied from the attestation.
    assert_eq!(
        (shard.epoch, shard.target_hash, shard.target_daa_score, shard.validator_set_commitment),
        (att.epoch, att.target_hash, att.target_daa_score, att.validator_set_commitment)
    );

    let tx = stake_attestation_shard_tx(&shard);
    assert!(tx.inputs.is_empty() && tx.outputs.is_empty());
    assert_eq!(dns_tx_kind(&tx.subnetwork_id), Some(DnsTxKind::StakeAttestationShard));
    // The built payload must pass the stateless shard validator and decode back.
    assert!(validate_stake_attestation_shard_payload(&tx.payload).is_ok());
    let decoded: StakeAttestationShardPayload = borsh::from_slice(&tx.payload).unwrap();
    assert_eq!(decoded, shard);
}

#[test]
fn dns_tx_kind_maps_overlay_subnetworks() {
    assert_eq!(dns_tx_kind(&SUBNETWORK_ID_STAKE_BOND), Some(DnsTxKind::StakeBond));
    assert_eq!(dns_tx_kind(&SUBNETWORK_ID_STAKE_ATTESTATION_SHARD), Some(DnsTxKind::StakeAttestationShard));
    assert_eq!(dns_tx_kind(&SUBNETWORK_ID_SLASHING_EVIDENCE), Some(DnsTxKind::SlashingEvidence));
    // Non-overlay subnetworks (native=0, coinbase=1, registry=2, unknown=3) → None.
    for b in [0u8, 1, 2, 3] {
        assert_eq!(dns_tx_kind(&SubnetworkId::from_byte(b)), None);
    }
    // dns_tx_kind agrees with the SubnetworkId::is_dns_overlay predicate.
    assert!(SUBNETWORK_ID_STAKE_BOND.is_dns_overlay());
    assert!(!SubnetworkId::from_byte(0).is_dns_overlay());
    // MISAKA Verified LLM Token-Weighted BFT subnets are routed and overlay-classified.
    assert_eq!(dns_tx_kind(&SUBNETWORK_ID_COMPUTE_CERTIFICATE), Some(DnsTxKind::ComputeCertificate));
    assert_eq!(dns_tx_kind(&SUBNETWORK_ID_COMPUTE_CHALLENGE), Some(DnsTxKind::ComputeChallenge));
    assert!(SUBNETWORK_ID_COMPUTE_CERTIFICATE.is_dns_overlay());
    assert!(SUBNETWORK_ID_COMPUTE_CHALLENGE.is_dns_overlay());
    // MISAKA §5 round 2. Both halves matter and they are separate code paths: `is_dns_overlay`
    // is what routes the tx at all (a `false` here means `SubnetworksDisabled` and precommits
    // never reach a block), while `dns_tx_kind` is what picks its stateless validator.
    assert_eq!(dns_tx_kind(&SUBNETWORK_ID_STAKE_PRECOMMIT), Some(DnsTxKind::StakePrecommit));
    assert!(SUBNETWORK_ID_STAKE_PRECOMMIT.is_dns_overlay());
}

// ---- MISAKA §5 round 2: the precommit payload ----

/// The lock is inside the signed digest. Without that the declaration would be an unsigned
/// field anyone could restate, and both the on-chain lock check and the cross-branch
/// equivocation proof would be checking a number nobody committed to. Same for the §5.1
/// snapshot commitment, at the denominator instead of the lock: a signature that did not
/// cover it could be counted against a `W(E)` its signer never weighed under.
#[test]
fn precommit_digest_covers_the_declared_lock() {
    let net = b"misaka-test";
    let op = TransactionOutpoint::new(Hash64::from_bytes([7; 64]), 0);
    let anchor = Hash64::from_bytes([0xa1; 64]);
    let snap = Hash64::from_bytes([0x5a; 64]);
    let base = stake_precommit_message(net, 11, anchor, 500, 10, Hash64::from_bytes([0xa0; 64]), snap, op);
    // Same vote, different declared lock ⇒ different digest ⇒ the old signature does not carry.
    assert_ne!(base, stake_precommit_message(net, 11, anchor, 500, 10, Hash64::from_bytes([0xbb; 64]), snap, op));
    assert_ne!(base, stake_precommit_message(net, 11, anchor, 500, 9, Hash64::from_bytes([0xa0; 64]), snap, op));
    // Same vote, different denominator ⇒ different digest — §5.1's whole point.
    assert_ne!(
        base,
        stake_precommit_message(net, 11, anchor, 500, 10, Hash64::from_bytes([0xa0; 64]), Hash64::from_bytes([0x5b; 64]), op)
    );
    // And it is bound to the network and the bond, like an attestation.
    assert_ne!(base, stake_precommit_message(b"other-net", 11, anchor, 500, 10, Hash64::from_bytes([0xa0; 64]), snap, op));
    assert_ne!(
        base,
        stake_precommit_message(
            net,
            11,
            anchor,
            500,
            10,
            Hash64::from_bytes([0xa0; 64]),
            snap,
            TransactionOutpoint::new(Hash64::from_bytes([8; 64]), 0)
        )
    );
}

/// A lock is something the signer held *before* the epoch it is locking, and "no lock" is
/// exactly `(0, zero)`. A half-declared lock is refused at the stateless layer: the chain
/// check could neither confirm nor contradict it.
#[test]
fn precommit_lock_must_be_whole_and_in_the_past() {
    let sig = vec![0u8; STAKE_ATTESTATION_SIG_LEN];
    let mk = |locked_epoch: u64, locked_hash: Hash64| StakePrecommitPayload {
        version: DNS_PAYLOAD_VERSION_V1,
        validator_id: Hash64::from_bytes([1; 64]),
        bond_outpoint: TransactionOutpoint::new(Hash64::from_bytes([1; 64]), 0),
        epoch: 10,
        target_hash: Hash64::from_bytes([0xa1; 64]),
        target_daa_score: 500,
        locked_epoch,
        locked_hash,
        snapshot_commitment: Hash64::default(),
        signature: sig.clone(),
    };
    let some = Hash64::from_bytes([0xa0; 64]);
    assert!(mk(0, Hash64::default()).lock_is_self_consistent(), "no lock at all is the first precommit's case");
    assert!(mk(9, some).lock_is_self_consistent());
    assert!(!mk(0, some).lock_is_self_consistent(), "an epoch-less lock is half-declared");
    assert!(!mk(9, Hash64::default()).lock_is_self_consistent(), "an anchor-less lock is half-declared");
    assert!(!mk(10, some).lock_is_self_consistent(), "a lock at the epoch it locks is not one the signer held");
    assert!(!mk(11, some).lock_is_self_consistent(), "nor is one from the future");

    assert_eq!(validate_stake_precommit_payload(&borsh::to_vec(&mk(10, some)).unwrap()), Err(DnsTxError::IncoherentPrecommitLock));
    assert!(validate_stake_precommit_payload(&borsh::to_vec(&mk(9, some)).unwrap()).is_ok());
}

/// Whatever heights a shipped preset carries, it is either dormant or configured for the
/// compute overlay it runs.
///
/// Each clause is a way a moved shadow fence could look right and run an overlay that does
/// nothing: an incoherent committee shape, an empty model table (no committee is ever drawn),
/// a walk too shallow to see a certificate leave its challenge window (no audit fee is ever
/// paid), or an unbonding window that lets a validator exit while its compute is still
/// challengeable. The weight fence has no clause: it is removed, and `validate_palw_v2`
/// refuses it anywhere but `u64::MAX`.
#[test]
fn shipped_presets_are_either_dormant_or_fully_forkable() {
    use crate::config::params::Params;
    use crate::network::NetworkType;
    // Checked on the MATERIALIZED presets, not on the raw consts. The model table cannot be a
    // `const` — its entries are keyed BLAKE2b digests — so it is attached at every
    // `From<NetworkType> for Params`, which is the form every consumer actually receives.
    // Reading the const here would fail a correctly-configured fork, and (worse) would pass
    // one whose table was attached in a single binary's argument parsing while simpa, the
    // harnesses and any embedder got a scheduled fence over nothing.
    let presets: Vec<(&str, DnsParams)> = [
        ("devnet", NetworkType::Devnet),
        ("simnet", NetworkType::Simnet),
        ("mainnet", NetworkType::Mainnet),
        ("testnet", NetworkType::Testnet),
    ]
    .into_iter()
    .filter_map(|(n, t)| Params::from(t).dns_params.map(|d| (n, d)))
    .collect();
    for (name, p) in presets {
        let dormant = p.vlt.vlt_shadow_activation_daa_score == u64::MAX && p.vlt.vlt_activation_daa_score == u64::MAX;
        if dormant {
            continue;
        }
        assert_eq!(p.vlt.vlt_activation_daa_score, u64::MAX, "{name}: the weight fence is removed");
        assert_eq!(p.vlt.is_coherent(), Ok(()), "{name}: a preset with a moved fence must be coherent");
        assert!(
            !p.vlt.model_cost_table.live().is_empty(),
            "{name}: fences moved with an empty model table — every job would mint zero and the fork would be a no-op"
        );
        assert!(
            p.vlt_credit_window_blue_score > p.vlt.challenge_window_blocks,
            "{name}: the compute-overlay walk cannot see a certificate leave its challenge window"
        );
        assert!(
            p.unbonding_period_blocks >= p.vlt.min_unbonding_period_blocks(p.attestation_epoch_length_blue_score),
            "{name}: §7 — a validator could exit while the compute it votes with is still challengeable"
        );
    }
}

// ---- MISAKA §5 round 2: what a broken lock costs ----

fn pcp(epoch: u64, anchor: u8, target_daa: u64, lock: (u64, u8)) -> StakePrecommitPayload {
    StakePrecommitPayload {
        version: DNS_PAYLOAD_VERSION_V1,
        validator_id: Hash64::from_bytes([1; 64]),
        bond_outpoint: TransactionOutpoint::new(Hash64::from_bytes([1; 64]), 0),
        epoch,
        target_hash: Hash64::from_bytes([anchor; 64]),
        target_daa_score: target_daa,
        locked_epoch: lock.0,
        locked_hash: if lock.1 == 0 { Hash64::default() } else { Hash64::from_bytes([lock.1; 64]) },
        snapshot_commitment: Hash64::default(),
        signature: vec![0u8; STAKE_ATTESTATION_SIG_LEN],
    }
}

/// The fault the two-round design exists to make provable — and, just as importantly, what it
/// deliberately does not call a fault.
///
/// A validator holds exactly one lock at a time, so two precommits declaring different anchors
/// at one `locked_epoch` are two histories, and two precommits disagreeing about one epoch are
/// the same. Both are decided from the payloads alone: no chain, no reachability, no access to
/// the branch either came from — which is the point, because the second one is typically from
/// a fork this node will never have the blocks for.
#[test]
fn precommit_evidence_convicts_only_what_two_payloads_prove() {
    use PrecommitFault::*;
    // Two anchors at one locked_epoch: the signer held two locks at one height.
    assert_eq!(precommit_fault(&pcp(11, 0xa1, 500, (10, 0xb0)), &pcp(12, 0xa2, 600, (10, 0xc0))), Some(ContradictoryLock));
    // Two answers for one epoch — anchor, or the lock held while signing.
    assert_eq!(precommit_fault(&pcp(11, 0xa1, 500, (10, 0xb0)), &pcp(11, 0xa2, 500, (10, 0xb0))), Some(Equivocation));
    assert_eq!(precommit_fault(&pcp(11, 0xa1, 500, (10, 0xb0)), &pcp(11, 0xa1, 500, (10, 0xc0))), Some(Equivocation));

    // A rebroadcast is not an offence.
    assert_eq!(precommit_fault(&pcp(11, 0xa1, 500, (10, 0xb0)), &pcp(11, 0xa1, 500, (10, 0xb0))), None);
    // An ordinary lock chain: each precommit declares its predecessor, so the locked_epochs
    // differ and nothing contradicts.
    assert_eq!(precommit_fault(&pcp(11, 0xa1, 500, (10, 0xb0)), &pcp(12, 0xa2, 600, (11, 0xa1))), None);
    // "No lock yet" is held by every validator exactly once and says nothing about history —
    // two first-precommits are not two locks.
    assert_eq!(precommit_fault(&pcp(11, 0xa1, 500, (0, 0)), &pcp(12, 0xa2, 600, (0, 0))), None);
    // Two different signers prove nothing about either.
    let other = StakePrecommitPayload { validator_id: Hash64::from_bytes([2; 64]), ..pcp(11, 0xa2, 500, (10, 0xc0)) };
    assert_eq!(precommit_fault(&pcp(11, 0xa1, 500, (10, 0xb0)), &other), None);

    // Re-declaring a lock the validator has moved past is a fault the WALK punishes (it stops
    // counting), but the two payloads alone cannot say which came first on chain, so it is not
    // proof of two histories and must not burn a bond.
    assert_eq!(precommit_fault(&pcp(11, 0xa1, 500, (10, 0xb0)), &pcp(12, 0xa2, 600, (10, 0xb0))), None);
}

/// The stateless layer accepts only evidence that is bound to one bond and genuinely
/// contradictory, and — like round 1's — carries no outputs, because the reporter reward is
/// minted at `(tx_id, 0)`.
#[test]
fn precommit_evidence_payload_shape_is_enforced() {
    let ev = |a: StakePrecommitPayload, b: StakePrecommitPayload| PrecommitEvidencePayload {
        version: DNS_PAYLOAD_VERSION_V1,
        bond_outpoint: TransactionOutpoint::new(Hash64::from_bytes([1; 64]), 0),
        precommit_a: a,
        precommit_b: b,
        reporter_reward_spk_payload: [0xab; 64],
    };
    let good = ev(pcp(11, 0xa1, 500, (10, 0xb0)), pcp(12, 0xa2, 600, (10, 0xc0)));
    assert!(validate_precommit_evidence_tx(&borsh::to_vec(&good).unwrap(), &[]).is_ok());

    // An output would collide with the reporter mint.
    let out = TransactionOutput::new(1, p2pkh_mldsa87_spk(&[0xab; 64]));
    assert_eq!(
        validate_precommit_evidence_tx(&borsh::to_vec(&good).unwrap(), &[out]),
        Err(DnsTxError::PrecommitEvidenceHasOutputs(1))
    );

    // A compatible pair proves nothing and must not reach the bond.
    let benign = ev(pcp(11, 0xa1, 500, (10, 0xb0)), pcp(12, 0xa2, 600, (11, 0xa1)));
    assert_eq!(
        validate_precommit_evidence_payload(&borsh::to_vec(&benign).unwrap()),
        Err(DnsTxError::PrecommitEvidenceNotContradictory)
    );

    // A precommit not bound to the bond the evidence names would let evidence be re-pointed.
    let elsewhere = StakePrecommitPayload {
        bond_outpoint: TransactionOutpoint::new(Hash64::from_bytes([9; 64]), 0),
        ..pcp(12, 0xa2, 600, (10, 0xc0))
    };
    assert_eq!(
        validate_precommit_evidence_payload(&borsh::to_vec(&ev(pcp(11, 0xa1, 500, (10, 0xb0)), elsewhere)).unwrap()),
        Err(DnsTxError::PrecommitEvidenceBondMismatch)
    );
}

/// Accepted precommit evidence burns the bond and pays the reporter, exactly as round 1's
/// does. Same offence, one round later — and a lock nobody can be burned for breaking is not
/// a lock.
#[test]
fn precommit_evidence_slashes_the_bond_and_pays_the_reporter() {
    let op = TransactionOutpoint::new(Hash64::from_bytes([0x11; 64]), 0);
    let view = ActiveBondView::from_records([(op, fixture_bond_record(op))]);
    let a = StakePrecommitPayload { bond_outpoint: op, ..pcp(11, 0xa1, 500, (10, 0xb0)) };
    let b = StakePrecommitPayload { bond_outpoint: op, ..pcp(12, 0xa2, 600, (10, 0xc0)) };
    let tx = precommit_evidence_tx(&PrecommitEvidencePayload {
        version: DNS_PAYLOAD_VERSION_V1,
        bond_outpoint: op,
        precommit_a: a,
        precommit_b: b,
        reporter_reward_spk_payload: [0xab; 64],
    });

    let muts = bond_mutations_from_accepted_txs(std::slice::from_ref(&tx), RESOLVE_DAA, 0, 0, false);
    assert_eq!(muts, vec![BondMutation::Slash(op, RESOLVE_DAA)], "the bond is slashed at acceptance");

    let effects = resolve_slashing_side_effects(std::slice::from_ref(&tx), &view, RESOLVE_DAA, 1000, 0, 0);
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].bond_outpoint, op);
    assert_eq!(effects[0].slashed_amount_sompi, 100_000_000_000);
    assert_eq!(effects[0].reporter_output.as_ref().unwrap().value, 10_000_000_000, "10% to the reporter, as in round 1");
    assert_eq!(effects[0].reporter_output.as_ref().unwrap().script_public_key, p2pkh_mldsa87_spk(&[0xab; 64]));
    assert_eq!(effects[0].burned_sompi, 90_000_000_000);
}

/// Every shipped preset weighs bonded stake under the graded φS rule: the weight fence is removed
/// (`u64::MAX` everywhere, refused otherwise), and only testnet runs the compute overlay.
#[test]
fn vlt_fence_keeps_shipped_presets_on_the_legacy_rule() {
    use crate::config::params::{GENESIS_ACTIVE_DNS_PARAMS, PRODUCTION_DNS_PARAMS, TESTNET_DNS_PARAMS};
    for (name, p) in
        [("genesis-active", GENESIS_ACTIVE_DNS_PARAMS), ("production", PRODUCTION_DNS_PARAMS), ("testnet", TESTNET_DNS_PARAMS)]
    {
        // The WEIGHT fence is removed: `u64::MAX` on every shipped preset, and
        // `validate_palw_v2` refuses any other value.
        assert_eq!(p.vlt.vlt_activation_daa_score, u64::MAX, "{name} must ship with VLT dormant");

        // The SHADOW fence is dormant everywhere EXCEPT where a release has deliberately
        // scheduled step 3 — testnet, at `TESTNET_VLT_SHADOW_FORK_DAA_SCORE`. Below that
        // height the preset is still byte-identical to the legacy rule, which is what this
        // test is really about; above it the overlay runs and is policed, with finality
        // untouched. A preset that scheduled its shadow fence is separately required to be
        // configured for the overlay by `shipped_presets_are_either_dormant_or_fully_forkable`.
        let shadow = p.vlt.vlt_shadow_activation_daa_score;
        if shadow == u64::MAX {
            assert!(!p.vlt_shadow_active_at(u64::MAX - 1), "{name}: no certificate is credited, no audit fee paid");
        } else if shadow == 0 {
            assert_eq!(name, "testnet", "only testnet has a scheduled shadow fork ({name} moved one unexpectedly)");
            // GENESIS-ACTIVE (the PALW re-genesis): there is no "below the fence" for the
            // legacy rule to hold in, because the fork ships in the genesis rules themselves.
            // That is only available to a chain being rebuilt — and it is why the rebuild took
            // it: no height to miss, no fleet to coordinate ahead of one, no window in which
            // the bond spend gate is off while challenge slashing is on.
            assert!(p.vlt_shadow_active_at(0), "{name}: genesis-active means the overlay runs from block 1");
        } else {
            assert_eq!(name, "testnet", "only testnet has a scheduled shadow fork ({name} moved one unexpectedly)");
            assert!(!p.vlt_shadow_active_at(shadow - 1), "{name}: the legacy rule holds right up to the fence");
            assert!(p.vlt_shadow_active_at(shadow), "{name}: and the overlay starts exactly at it");
        }
        // The removed token overlay's fences stay dormant on every preset (`validate_palw_v2`
        // refuses anything else), and the dormant preset is internally coherent.
        assert_eq!(p.tkn.tkn_activation_daa_score, u64::MAX, "{name} must ship with the token program dormant");
        assert!(!p.tkn.shadow_active_at(u64::MAX - 1), "{name}: the token fences never open");
        assert!(p.tkn.is_coherent().is_ok(), "{name}: the dormant token preset must be coherent");
        assert!(
            matches!(p.epoch_credit_rule(), EpochCreditRule::QualityFloor { quality_floor_bps } if quality_floor_bps == p.stake_event_quality_floor_bps),
            "{name} must stay on the graded φS rule"
        );
        // The bond requirement is untouched by the VLT work — VLT changes what a bond BUYS
        // (weight capped at `lambda x collateral`), never what it costs to hold one. The
        // amounts are 20,000,000 MSK per validator over a floor of 6, whose product is the
        // active-stake gate (the user's 2026-09-25 decision, the 2026-09-24 mainnet assumption).
        // From 2026-08-30 to then the bond was 10,000 MSK over 12, tied to the figure a PALW
        // genesis seat posted (ADR-0061); the decision separated the two floors again — a DNS
        // vote can reverse a confirmed anchor, a PALW seat backs its own claims — so the tie to
        // `GENESIS_BOND_COLLATERAL_SOMPI` is gone. The derivation lives in
        // `the_active_stake_gate_is_the_product_of_the_two_floors` and on
        // `PRODUCTION_DNS_PARAMS.min_bond_amount_sompi`.
        if name != "genesis-active" && name != "testnet" {
            assert_ne!(
                p.min_bond_amount_sompi,
                crate::config::premine::GENESIS_BOND_COLLATERAL_SOMPI,
                "{name}: a validator bond is its own floor again, not the PALW genesis seat's"
            );
            assert_eq!(p.min_bond_amount_sompi, 20_000_000 * crate::constants::SOMPI_PER_KASPA, "{name}");
            assert_eq!(p.min_active_validators, 6, "{name}");
            assert_eq!(
                p.min_active_stake_sompi,
                p.min_active_validators as u64 * p.min_bond_amount_sompi,
                "{name}: the stake gate is the product of the two floors"
            );
        }
    }
}

/// Health must not shift under the φS rule. The boundary case is
/// `f == φS` exactly: it earns ZERO StakeScore credit (the graded numerator vanishes there)
/// yet has always reported `Active`, so a credit-based health test would silently change the
/// signal on live networks. Pin both halves.
#[test]
fn dns_health_preserves_the_legacy_phi_s_boundary() {
    let rule = EpochCreditRule::QualityFloor { quality_floor_bps: 6000 };
    let at_floor = EpochStakeTally { epoch: 0, signed_weight: 6, total_weight: 10 }; // f == φS
    assert_eq!(epoch_credit(6, 10, rule), 0, "exactly at φS earns no credit");
    assert_eq!(
        derive_dns_health(&[at_floor, at_floor, at_floor], rule, 1000, 3, true),
        DnsHealth::Active,
        "but health has always read exactly-at-φS as Active"
    );
    // Just below φS is genuinely degraded.
    let below = EpochStakeTally { epoch: 0, signed_weight: 5, total_weight: 10 };
    assert_eq!(derive_dns_health(&[below, below, below], rule, 1000, 3, true), DnsHealth::DegradedStakeQualityLow);
}

fn fixture_verdict(id: u64, verdict: crate::vlt::VerificationVerdict, replay: Hash64) -> crate::vlt::VerifierAttestation {
    crate::vlt::VerifierAttestation {
        version: DNS_PAYLOAD_VERSION_V1,
        verifier_id: Hash64::from_u64_word(id),
        bond_outpoint: TransactionOutpoint::new(Hash64::from_u64_word(id + 100), 0),
        verdict,
        replay_receipt_hash: replay,
        signature: vec![0u8; STAKE_ATTESTATION_SIG_LEN],
    }
}

fn fixture_certificate() -> ComputeCertificatePayload {
    ComputeCertificatePayload {
        version: DNS_PAYLOAD_VERSION_V1,
        epoch: 7,
        executor_id: Hash64::from_u64_word(1),
        executor_bond_outpoint: TransactionOutpoint::new(Hash64::from_u64_word(2), 0),
        commitment_tx_id: Hash64::from_u64_word(1234),
        spec: crate::vlt::LlmJobSpec {
            version: DNS_PAYLOAD_VERSION_V1,
            model_weights_hash: Hash64::from_u64_word(10),
            runtime_hash: Hash64::from_u64_word(11),
            quantization: crate::vlt::QuantizationProfile::Bf16,
            input_commitment: Hash64::from_u64_word(12),
            sampling_seed: [3u8; 32],
            max_tokens: 4096,
            verification_scheme: crate::vlt::VerificationScheme::CanonicalFullReplay,
        },
        receipt: crate::vlt::ComputeReceipt {
            version: DNS_PAYLOAD_VERSION_V1,
            output_commitment: Hash64::from_u64_word(13),
            prefill_tokens: 100,
            decode_tokens: 20,
            trace_commitment: Hash64::from_u64_word(14),
        },
        executor_signature: vec![0u8; STAKE_ATTESTATION_SIG_LEN],
    }
}

const FIXTURE_JOB_ID: u64 = 501;
/// The seed of the execution the fixture certificate claims.
const FIXTURE_EXECUTOR_SEED: u64 = 99;

/// A replay proof for the execution identified by `seed` — internally consistent, exactly as a
/// real one is, so the fixture verdicts have to satisfy the same rule real ones do.
fn fixture_proof(seed: u64) -> crate::vlt::ReplayProof {
    let residuals = crate::vlt::ReplayResiduals { job_nullifier: Hash64::from_u64_word(seed), ..fixture_residuals() };
    let receipt = crate::vlt::ComputeReceipt {
        version: DNS_PAYLOAD_VERSION_V1,
        output_commitment: Hash64::from_u64_word(seed),
        prefill_tokens: 100,
        decode_tokens: 20,
        trace_commitment: crate::vlt::residual_commitment(&residuals),
    };
    crate::vlt::ReplayProof { receipt, residuals }
}

/// The `R_j` that execution produces — what a verdict over it must declare.
fn fixture_hash(seed: u64) -> Hash64 {
    crate::vlt::compute_receipt_hash_for_job(Hash64::from_u64_word(FIXTURE_JOB_ID), &fixture_proof(seed).receipt)
}

/// The residual set whose fold is the fixture trace commitment — what an honest replay reveals.
fn fixture_residuals() -> crate::vlt::ReplayResiduals {
    crate::vlt::ReplayResiduals {
        job_nullifier: Hash64::from_u64_word(201),
        request_commitment: Hash64::from_u64_word(202),
        model_profile_id: Hash64::from_u64_word(203),
        runtime_class_id: Hash64::from_u64_word(204),
        runtime_manifest_hash: Hash64::from_u64_word(205),
        shape_profile_id: Hash64::from_u64_word(206),
        cu_ruleset_id: Hash64::from_u64_word(207),
        canonical_compute_units: 41_692,
        operation_schedule_commitment: Hash64::from_u64_word(208),
        schedule_event_count: 80,
        trace_scheme_id: Hash64::from_u64_word(209),
        gemm_trace_root: Hash64::from_u64_word(210),
        trace_event_count: 2_466,
    }
}

/// A verdict whose replay proof really produces the hash it reports, for the execution `seed`.
/// `seed == FIXTURE_EXECUTOR_SEED` is a reproduction of the certificate; anything else diverges.
fn fixture_verdict_payload(verifier: u64, verdict: crate::vlt::VerificationVerdict, seed: u64) -> ComputeVerdictPayload {
    ComputeVerdictPayload {
        version: DNS_PAYLOAD_VERSION_V1,
        certificate_tx_id: Hash64::from_u64_word(500),
        job_id: Hash64::from_u64_word(FIXTURE_JOB_ID),
        executor_receipt_hash: fixture_hash(FIXTURE_EXECUTOR_SEED),
        verifier_id: Hash64::from_u64_word(verifier),
        bond_outpoint: TransactionOutpoint::new(Hash64::from_u64_word(verifier + 100), 0),
        verdict,
        replay_receipt_hash: fixture_hash(seed),
        replay_proof: fixture_proof(seed),
        signature: vec![0u8; STAKE_ATTESTATION_SIG_LEN],
    }
}

#[test]
fn validate_compute_certificate_accepts_wellformed() {
    let ok = fixture_certificate();
    assert_eq!(validate_compute_certificate_payload(&borsh::to_vec(&ok).unwrap()), Ok(()));

    // Reserved schemes are not consensus-eligible in v0.1.
    let mut reserved = fixture_certificate();
    reserved.spec.verification_scheme = crate::vlt::VerificationScheme::SuccinctProof;
    assert_eq!(
        validate_compute_certificate_payload(&borsh::to_vec(&reserved).unwrap()),
        Err(DnsTxError::IneligibleVerificationScheme)
    );

    // Wrong ML-DSA-87 signature width.
    let mut short_sig = fixture_certificate();
    short_sig.executor_signature = vec![0u8; 10];
    assert_eq!(validate_compute_certificate_payload(&borsh::to_vec(&short_sig).unwrap()), Err(DnsTxError::InvalidSignatureLen(10)));

    assert_eq!(validate_compute_certificate_payload(b"not borsh"), Err(DnsTxError::Decode));
}

/// A verdict must agree with its own two receipt hashes. A `Confirmed` over a hash the
/// verifier did not reproduce (or a `Refuted` over one it did) is a lie visible in the
/// payload alone, so it is refused at the stateless layer rather than counted and argued
/// about later.
#[test]
fn validate_compute_verdict_requires_self_consistency() {
    use crate::vlt::VerificationVerdict::{Confirmed, Refuted};
    let executor_hash = FIXTURE_EXECUTOR_SEED;
    let other = 98u64;

    let honest_confirm = fixture_verdict_payload(21, Confirmed, executor_hash);
    assert_eq!(validate_compute_verdict_payload(&borsh::to_vec(&honest_confirm).unwrap()), Ok(()));
    let honest_refute = fixture_verdict_payload(21, Refuted, other);
    assert_eq!(validate_compute_verdict_payload(&borsh::to_vec(&honest_refute).unwrap()), Ok(()));

    // "I confirm" while reporting a different replay result.
    let liar = fixture_verdict_payload(21, Confirmed, other);
    assert_eq!(validate_compute_verdict_payload(&borsh::to_vec(&liar).unwrap()), Err(DnsTxError::SelfContradictoryVerdict));
    // "I refute" while reporting the same result — a griefing refutation.
    let griefer = fixture_verdict_payload(21, Refuted, executor_hash);
    assert_eq!(validate_compute_verdict_payload(&borsh::to_vec(&griefer).unwrap()), Err(DnsTxError::SelfContradictoryVerdict));

    // A confirmation whose proof does not produce the hash it reports never ran the job.
    let mut fabricated = honest_confirm.clone();
    fabricated.replay_proof = fixture_proof(other);
    assert_eq!(validate_compute_verdict_payload(&borsh::to_vec(&fabricated).unwrap()), Err(DnsTxError::SelfContradictoryVerdict));
    // …and a refutation is no longer free either: it must exhibit the execution it claims.
    let mut baseless = honest_refute.clone();
    baseless.replay_proof = fixture_proof(FIXTURE_EXECUTOR_SEED);
    assert_eq!(validate_compute_verdict_payload(&borsh::to_vec(&baseless).unwrap()), Err(DnsTxError::SelfContradictoryVerdict));

    let mut short_sig = honest_confirm.clone();
    short_sig.signature = vec![0u8; 4];
    assert_eq!(validate_compute_verdict_payload(&borsh::to_vec(&short_sig).unwrap()), Err(DnsTxError::InvalidSignatureLen(4)));
    assert_eq!(validate_compute_verdict_payload(b"not borsh"), Err(DnsTxError::Decode));
}

/// The job input rides the commitment so a sortitioned verifier can replay from chain data
/// alone. It is the only unbounded field in the compute overlay, so its size is the only
/// thing the stateless layer can — and must — decide about it.
#[test]
fn validate_compute_commitment_bounds_the_published_job_input() {
    let fixture = |input: Vec<u8>| ComputeCommitmentPayload {
        version: DNS_PAYLOAD_VERSION_V1,
        job_id: Hash64::from_u64_word(501),
        executor_id: Hash64::from_u64_word(11),
        executor_bond_outpoint: TransactionOutpoint::new(Hash64::from_u64_word(111), 0),
        input,
        signature: vec![0u8; STAKE_ATTESTATION_SIG_LEN],
    };

    let ok = fixture(b"the capital of France is".to_vec());
    assert_eq!(validate_compute_commitment_payload(&borsh::to_vec(&ok).unwrap()), Ok(()));
    let at_limit = fixture(vec![0u8; MAX_JOB_INPUT_BYTES]);
    assert_eq!(validate_compute_commitment_payload(&borsh::to_vec(&at_limit).unwrap()), Ok(()));

    // Nothing to replay is not a job.
    assert_eq!(validate_compute_commitment_payload(&borsh::to_vec(&fixture(Vec::new())).unwrap()), Err(DnsTxError::JobInputSize(0)));
    let over = fixture(vec![0u8; MAX_JOB_INPUT_BYTES + 1]);
    assert_eq!(
        validate_compute_commitment_payload(&borsh::to_vec(&over).unwrap()),
        Err(DnsTxError::JobInputSize(MAX_JOB_INPUT_BYTES + 1))
    );

    let mut short_sig = ok.clone();
    short_sig.signature = vec![0u8; 7];
    assert_eq!(validate_compute_commitment_payload(&borsh::to_vec(&short_sig).unwrap()), Err(DnsTxError::InvalidSignatureLen(7)));
    assert_eq!(validate_compute_commitment_payload(b"not borsh"), Err(DnsTxError::Decode));
}

#[test]
fn validate_compute_challenge_requires_a_real_contradiction_and_no_outputs() {
    let bond = TransactionOutpoint::new(Hash64::from_u64_word(121), 0);
    let a = fixture_verdict(21, crate::vlt::VerificationVerdict::Confirmed, Hash64::from_u64_word(99));
    let b = fixture_verdict(21, crate::vlt::VerificationVerdict::Refuted, Hash64::from_u64_word(98));
    let base = ComputeChallengePayload {
        version: DNS_PAYLOAD_VERSION_V1,
        certificate_tx_id: Hash64::from_u64_word(5),
        job_id: Hash64::from_u64_word(6),
        executor_receipt_hash: Hash64::from_u64_word(99),
        kind: ComputeFraudKind::ContradictoryVerification,
        challenger_id: Hash64::from_u64_word(7),
        challenger_bond_outpoint: TransactionOutpoint::new(Hash64::from_u64_word(8), 0),
        target_bond_outpoint: bond,
        replay_receipt_hash: Hash64::from_u64_word(99),
        contradictory_verdicts: vec![a.clone(), b.clone()],
        signature: vec![0u8; STAKE_ATTESTATION_SIG_LEN],
        reporter_reward_spk_payload: [0u8; 64],
    };
    assert_eq!(validate_compute_challenge_payload(&borsh::to_vec(&base).unwrap()), Ok(()));

    // Two identical verdicts are a duplicate, not a contradiction.
    let mut same = base.clone();
    same.contradictory_verdicts = vec![a.clone(), a.clone()];
    assert_eq!(validate_compute_challenge_payload(&borsh::to_vec(&same).unwrap()), Err(DnsTxError::MalformedContradictionProof));

    // Two different verifiers disagreeing is normal, not an offence by either.
    let mut two_verifiers = base.clone();
    two_verifiers.contradictory_verdicts[1].verifier_id = Hash64::from_u64_word(22);
    assert_eq!(
        validate_compute_challenge_payload(&borsh::to_vec(&two_verifiers).unwrap()),
        Err(DnsTxError::MalformedContradictionProof)
    );

    // A contradiction proof may not be aimed at a third party's bond.
    let mut wrong_target = base.clone();
    wrong_target.target_bond_outpoint = TransactionOutpoint::new(Hash64::from_u64_word(999), 0);
    assert_eq!(
        validate_compute_challenge_payload(&borsh::to_vec(&wrong_target).unwrap()),
        Err(DnsTxError::MalformedContradictionProof)
    );

    // Other kinds carry no verdicts.
    let mut forged = base.clone();
    forged.kind = ComputeFraudKind::ForgedReceipt;
    assert_eq!(validate_compute_challenge_payload(&borsh::to_vec(&forged).unwrap()), Err(DnsTxError::MalformedContradictionProof));
    forged.contradictory_verdicts.clear();
    assert_eq!(validate_compute_challenge_payload(&borsh::to_vec(&forged).unwrap()), Ok(()));

    // Pure evidence carrier: no outputs, so the reporter mint at (tx, 0) cannot collide.
    let bytes = borsh::to_vec(&forged).unwrap();
    assert_eq!(validate_compute_challenge_tx(&bytes, &[]), Ok(()));
    let out = TransactionOutput::new(1, crate::tx::ScriptPublicKey::new(0, crate::tx::ScriptVec::from_slice(&[0x51])));
    assert_eq!(validate_compute_challenge_tx(&bytes, &[out]), Err(DnsTxError::SlashingEvidenceHasOutputs(1)));
}

/// Only an **objectively provable** offence may slash at acceptance.
///
/// `ContradictoryVerification` is proved by two of the accused's own signatures, so it slashes.
/// `ForgedReceipt` is the claim "I re-ran it and got something else" — no validator can check
/// that without re-running the job, so it must NOT slash: were it to, any bonded party could
/// burn any other party's stake with one transaction and no evidence. It still denies the
/// certificate its credit, which is the part that is decidable from chain data.
#[test]
fn only_a_provable_compute_offence_slashes() {
    let target = TransactionOutpoint::new(Hash64::from_u64_word(121), 0);
    let challenge = |kind| ComputeChallengePayload {
        version: DNS_PAYLOAD_VERSION_V1,
        certificate_tx_id: Hash64::from_u64_word(5),
        job_id: Hash64::from_u64_word(6),
        executor_receipt_hash: Hash64::from_u64_word(99),
        kind,
        challenger_id: Hash64::from_u64_word(7),
        challenger_bond_outpoint: TransactionOutpoint::new(Hash64::from_u64_word(8), 0),
        target_bond_outpoint: target,
        replay_receipt_hash: Hash64::from_u64_word(42),
        contradictory_verdicts: vec![],
        signature: vec![0u8; STAKE_ATTESTATION_SIG_LEN],
        reporter_reward_spk_payload: [0u8; 64],
    };
    let tx = |kind| {
        Transaction::new(
            crate::constants::TX_VERSION,
            vec![],
            vec![],
            0,
            SUBNETWORK_ID_COMPUTE_CHALLENGE,
            0,
            borsh::to_vec(&challenge(kind)).unwrap(),
        )
    };
    assert_eq!(
        bond_mutations_from_accepted_txs(&[tx(ComputeFraudKind::ContradictoryVerification)], 4_242, 0, 0, false),
        vec![BondMutation::Slash(target, 4_242)]
    );
    for unprovable in [ComputeFraudKind::ForgedReceipt, ComputeFraudKind::InvalidCertificate, ComputeFraudKind::FailedChallenge] {
        assert!(
            bond_mutations_from_accepted_txs(&[tx(unprovable)], 4_242, 0, 0, false).is_empty(),
            "{unprovable:?} is a claim, not a proof, and must not slash"
        );
    }

    // A compute CERTIFICATE, by contrast, never touches the bond registry — it mints credit.
    let cert_tx = Transaction::new(
        crate::constants::TX_VERSION,
        vec![],
        vec![],
        0,
        SUBNETWORK_ID_COMPUTE_CERTIFICATE,
        0,
        borsh::to_vec(&fixture_certificate()).unwrap(),
    );
    assert!(bond_mutations_from_accepted_txs(&[cert_tx], 4_242, 0, 0, false).is_empty());
}

#[test]
fn validate_stake_bond_payload_accepts_wellformed() {
    let bytes = borsh::to_vec(&fixture_bond()).unwrap();
    assert_eq!(validate_stake_bond_payload(&bytes), Ok(()));
}

#[test]
fn validate_stake_bond_payload_rejects_malformed() {
    // Undecodable bytes.
    assert_eq!(validate_stake_bond_payload(&[0xff, 0x00, 0x12]), Err(DnsTxError::Decode));
    // Bad version.
    let mut bad = fixture_bond();
    bad.version = 2;
    assert_eq!(validate_stake_bond_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::UnsupportedVersion(2)));
    // Zero bonded amount.
    let mut bad = fixture_bond();
    bad.amount = 0;
    assert_eq!(validate_stake_bond_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::ZeroBondAmount));
    // Wrong validator pubkey length.
    let mut bad = fixture_bond();
    bad.validator_pubkey = vec![0u8; STAKE_VALIDATOR_PUBKEY_LEN - 1];
    assert_eq!(
        validate_stake_bond_payload(&borsh::to_vec(&bad).unwrap()),
        Err(DnsTxError::InvalidPubKeyLen(STAKE_VALIDATOR_PUBKEY_LEN - 1))
    );
    // audit H-04: validator_pubkey_hash not derived from the validator key.
    let mut bad = fixture_bond();
    bad.validator_pubkey_hash = Hash64::from_bytes([0x01u8; 64]);
    assert_eq!(validate_stake_bond_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::ValidatorPubkeyHashMismatch));
}

// ---- kaspa-pq H-05: StakeUnbondRequest (audit) ----

fn fixture_unbond(bond_outpoint: TransactionOutpoint) -> StakeUnbondRequestPayload {
    StakeUnbondRequestPayload {
        version: DNS_PAYLOAD_VERSION_V1,
        bond_outpoint,
        owner_pubkey: vec![0xccu8; STAKE_VALIDATOR_PUBKEY_LEN],
        signature: vec![0u8; STAKE_ATTESTATION_SIG_LEN],
    }
}

#[test]
fn validate_stake_unbond_payload_accepts_and_rejects() {
    let op = TransactionOutpoint::new(Hash64::from_bytes([0x11u8; 64]), 0);
    assert_eq!(validate_stake_unbond_payload(&borsh::to_vec(&fixture_unbond(op)).unwrap()), Ok(()));
    assert_eq!(validate_stake_unbond_payload(&[0xff, 0x00]), Err(DnsTxError::Decode));
    let mut bad = fixture_unbond(op);
    bad.version = 2;
    assert_eq!(validate_stake_unbond_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::UnsupportedVersion(2)));
    let mut bad = fixture_unbond(op);
    bad.owner_pubkey = vec![0u8; 10];
    assert_eq!(validate_stake_unbond_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::InvalidPubKeyLen(10)));
    let mut bad = fixture_unbond(op);
    bad.signature = vec![0u8; 10];
    assert_eq!(validate_stake_unbond_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::InvalidSignatureLen(10)));
}

#[test]
fn dns_tx_kind_maps_stake_unbond() {
    assert_eq!(dns_tx_kind(&SUBNETWORK_ID_STAKE_UNBOND), Some(DnsTxKind::StakeUnbond));
}

/// Incident 2026-08-07 — the own-body/mergeset authorization gap. `unbond_request_authorized`
/// is a block-validity gate over the block's OWN body, but bond mutations come from a chain
/// block's ACCEPTED txs, which include everything its mergeset merges. A forged request that
/// rides in a merge-blue block therefore reached the bond store unchecked: observed live, a
/// request signed by a NON-owner was correctly disqualified as a chain block and the victim's
/// bond still moved to `Unbonding`, persisting across restart on both nodes.
///
/// Past the fence the mutation carries the requester's claimed `validator_id` and the view
/// binds it to the record, so a foreign request is inert.
#[test]
fn foreign_unbond_request_cannot_stamp_a_bond_past_the_fence() {
    let op = TransactionOutpoint::new(Hash64::from_bytes([0x22u8; 64]), 0);
    let payload = borsh::to_vec(&fixture_unbond(op)).unwrap();
    let tx = Transaction::new(crate::constants::TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_STAKE_UNBOND, 0, payload);
    let requester = validator_id_from_pubkey(&vec![0xccu8; STAKE_VALIDATOR_PUBKEY_LEN]);

    // Below the fence the mutation is unbound — byte-identical to the pre-fix behaviour.
    assert_eq!(
        bond_mutations_from_accepted_txs(std::slice::from_ref(&tx), 5_000, 0, 0, false),
        vec![BondMutation::Unbond(op, 5_000, None)]
    );
    // Past it the mutation carries the requester identity.
    let muts = bond_mutations_from_accepted_txs(&[tx], 5_000, 0, 0, true);
    assert_eq!(muts, vec![BondMutation::Unbond(op, 5_000, Some(requester))]);

    // A bond owned by SOMEONE ELSE is untouched by the request.
    let victim = StakeBondRecord { owner_pubkey_hash: Hash64::from_bytes([0x77u8; 64]), ..mk_bond(1, 10) };
    assert_ne!(victim.owner_pubkey_hash, requester, "fixture must model a foreign owner");
    let mut view = ActiveBondView::from_records(vec![(op, victim.clone())]);
    view.apply(&muts);
    assert_eq!(view.get(&op).unwrap().unbond_request_daa_score, None, "a foreign request must not start the unbond clock");

    // The bond's real owner is unaffected by the guard.
    let owned = StakeBondRecord { owner_pubkey_hash: requester, ..victim.clone() };
    let mut view = ActiveBondView::from_records(vec![(op, owned)]);
    view.apply(&muts);
    assert_eq!(view.get(&op).unwrap().unbond_request_daa_score, Some(5_000), "the owner's own request still applies");

    // Revert is the exact mirror of apply, so a reorg cannot desync the store.
    view.revert(&muts);
    assert_eq!(view.get(&op).unwrap().unbond_request_daa_score, None);
    let mut view = ActiveBondView::from_records(vec![(op, victim)]);
    view.apply(&muts);
    view.revert(&muts);
    assert_eq!(view.get(&op).unwrap().unbond_request_daa_score, None, "reverting a never-applied stamp is a no-op");
}

/// Every shipped preset must have the unbond-authorization rule GENESIS-ACTIVE, so a new
/// network can never silently reopen the own-body/mergeset gap by forgetting to pick a score.
#[test]
fn presets_declare_the_unbond_authz_fence() {
    use crate::config::params::{GENESIS_ACTIVE_DNS_PARAMS, PRODUCTION_DNS_PARAMS, TESTNET_DNS_PARAMS};
    for (name, p) in [
        ("genesis-active/dev-sim", GENESIS_ACTIVE_DNS_PARAMS),
        ("production/mainnet", PRODUCTION_DNS_PARAMS),
        ("testnet", TESTNET_DNS_PARAMS),
    ] {
        assert_eq!(
            p.unbond_authz_mergeset_activation_daa_score, 0,
            "{name} must keep unbond authorization genesis-active — a per-net activation score is a footgun the next testnet would inherit"
        );
    }
}

#[test]
fn unbond_mutation_and_view_apply_revert() {
    let op = TransactionOutpoint::new(Hash64::from_bytes([0x22u8; 64]), 0);
    let payload = borsh::to_vec(&fixture_unbond(op)).unwrap();
    let tx = Transaction::new(crate::constants::TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_STAKE_UNBOND, 0, payload);
    // bond_mutations derives exactly one Unbond stamped at the accepting DAA.
    let muts = bond_mutations_from_accepted_txs(&[tx], 5_000, 0, 0, false);
    assert_eq!(muts, vec![BondMutation::Unbond(op, 5_000, None)]);
    // apply → unbond clock set → effective status Unbonding (precedence over activation).
    let mut view = ActiveBondView::from_records([(op, stake_bond_record_from_payload(&fixture_bond(), op))]);
    view.apply(&muts);
    assert_eq!(view.get(&op).unwrap().unbond_request_daa_score, Some(5_000));
    assert_eq!(effective_bond_status(view.get(&op).unwrap(), 5_000), BondStatus::Unbonding);
    // revert → cleared (clean, because the rule admits at most one unbond per bond).
    view.revert(&muts);
    assert_eq!(view.get(&op).unwrap().unbond_request_daa_score, None);
}

// ---- ADR-0016 D.1: validate_stake_bond_tx (stake-lock rule) ----

#[test]
fn validate_stake_bond_tx_accepts_locked_output() {
    let bond = fixture_bond();
    let bytes = borsh::to_vec(&bond).unwrap();
    let outputs = vec![TransactionOutput::new(bond.amount, p2pkh_mldsa87_spk(&bond.owner_reward_spk_payload))];
    assert_eq!(validate_stake_bond_tx(&bytes, &outputs), Ok(()));
}

#[test]
fn validate_stake_bond_tx_rejects_unlocked_or_misdirected() {
    let bond = fixture_bond();
    let bytes = borsh::to_vec(&bond).unwrap();
    // No output-0 to lock the stake in.
    assert_eq!(validate_stake_bond_tx(&bytes, &[]), Err(DnsTxError::MissingBondOutput));
    // Output-0 value != amount.
    let wrong_value = vec![TransactionOutput::new(bond.amount - 1, p2pkh_mldsa87_spk(&bond.owner_reward_spk_payload))];
    assert_eq!(
        validate_stake_bond_tx(&bytes, &wrong_value),
        Err(DnsTxError::BondOutputValueMismatch { expected: bond.amount, got: bond.amount - 1 })
    );
    // Correct value but the output pays a different (non-owner) payload.
    let wrong_spk = vec![TransactionOutput::new(bond.amount, p2pkh_mldsa87_spk(&[0x00u8; 64]))];
    assert_eq!(validate_stake_bond_tx(&bytes, &wrong_spk), Err(DnsTxError::BondOutputScriptMismatch));
    // Payload checks still fire first (zero amount) regardless of outputs.
    let mut zero = bond.clone();
    zero.amount = 0;
    assert_eq!(validate_stake_bond_tx(&borsh::to_vec(&zero).unwrap(), &[]), Err(DnsTxError::ZeroBondAmount));
}

#[test]
fn validate_attestation_shard_accepts_wellformed() {
    // The MAX-sized shard and the single-attestation lower bound both pass.
    assert_eq!(validate_stake_attestation_shard_payload(&borsh::to_vec(&fixture_shard(MAX_ATTESTATIONS_PER_SHARD)).unwrap()), Ok(()));
    assert_eq!(validate_stake_attestation_shard_payload(&borsh::to_vec(&fixture_shard(1)).unwrap()), Ok(()));
}

#[test]
fn validate_attestation_shard_rejects_malformed() {
    // Undecodable.
    assert_eq!(validate_stake_attestation_shard_payload(&[0x00]), Err(DnsTxError::Decode));
    // Empty shard.
    assert_eq!(validate_stake_attestation_shard_payload(&borsh::to_vec(&fixture_shard(0)).unwrap()), Err(DnsTxError::EmptyShard));
    // Over the cardinality cap.
    let over = MAX_ATTESTATIONS_PER_SHARD + 1;
    assert_eq!(
        validate_stake_attestation_shard_payload(&borsh::to_vec(&fixture_shard(over)).unwrap()),
        Err(DnsTxError::ShardTooLarge(over))
    );
    // Bad shard version.
    let mut bad = fixture_shard(2);
    bad.version = 9;
    assert_eq!(validate_stake_attestation_shard_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::UnsupportedVersion(9)));
    // Member attestation with wrong signature length.
    let mut bad = fixture_shard(2);
    bad.attestations[1].signature = vec![0u8; STAKE_ATTESTATION_SIG_LEN + 1];
    assert_eq!(
        validate_stake_attestation_shard_payload(&borsh::to_vec(&bad).unwrap()),
        Err(DnsTxError::InvalidSignatureLen(STAKE_ATTESTATION_SIG_LEN + 1))
    );
    // Member attestation that disagrees with the shard's anchor hash.
    let mut bad = fixture_shard(2);
    bad.attestations[1].target_hash = Hash64::from_bytes([0xee; 64]);
    assert_eq!(validate_stake_attestation_shard_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::ShardTupleMismatch));
    // Member attestation whose epoch disagrees.
    let mut bad = fixture_shard(2);
    bad.attestations[0].epoch = 999;
    assert_eq!(validate_stake_attestation_shard_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::ShardTupleMismatch));
    // MISAKA VLT PR 2: a non-zero validator_set_commitment is stateless-VALID — the zero rule
    // lives at the acceptance gate, not here. It remains part of the shard tuple, so members
    // must agree on it.
    let mut shard = fixture_shard(2);
    shard.validator_set_commitment = Hash64::from_bytes([0x01; 64]);
    for att in shard.attestations.iter_mut() {
        att.validator_set_commitment = Hash64::from_bytes([0x01; 64]);
    }
    assert_eq!(validate_stake_attestation_shard_payload(&borsh::to_vec(&shard).unwrap()), Ok(()));
    let mut bad = shard.clone();
    bad.attestations[1].validator_set_commitment = Hash64::default();
    assert_eq!(validate_stake_attestation_shard_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::ShardTupleMismatch));
}

#[test]
fn validate_slashing_evidence_accepts_wellformed() {
    assert_eq!(validate_slashing_evidence_payload(&borsh::to_vec(&fixture_evidence()).unwrap()), Ok(()));
}

#[test]
fn validate_slashing_evidence_rejects_malformed() {
    // Undecodable.
    assert_eq!(validate_slashing_evidence_payload(&[0x01, 0x02]), Err(DnsTxError::Decode));
    // Bad version.
    let mut bad = fixture_evidence();
    bad.version = 5;
    assert_eq!(validate_slashing_evidence_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::UnsupportedVersion(5)));
    // Same anchor → not equivocation.
    let mut bad = fixture_evidence();
    bad.attestation_b.target_hash = bad.attestation_a.target_hash;
    assert_eq!(validate_slashing_evidence_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::EvidenceNotIncompatible));
    // Different validator_id → not the same triple.
    let mut bad = fixture_evidence();
    bad.attestation_b.validator_id = Hash64::from_bytes([0x5a; 64]);
    assert_eq!(validate_slashing_evidence_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::EvidenceTripleMismatch));
    // Different epoch → not the same triple.
    let mut bad = fixture_evidence();
    bad.attestation_b.epoch += 1;
    assert_eq!(validate_slashing_evidence_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::EvidenceTripleMismatch));
    // Payload bond_outpoint that does not match the cited attestations.
    let mut bad = fixture_evidence();
    bad.bond_outpoint = TransactionOutpoint::new(Hash64::from_bytes([0x01; 64]), 0);
    assert_eq!(validate_slashing_evidence_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::EvidenceTripleMismatch));
    // Bad signature length in the first attestation.
    let mut bad = fixture_evidence();
    bad.attestation_a.signature = vec![0u8; 10];
    assert_eq!(validate_slashing_evidence_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::InvalidSignatureLen(10)));
}

#[test]
fn dns_tx_error_display_is_nonempty() {
    for e in [
        DnsTxError::Decode,
        DnsTxError::UnsupportedVersion(2),
        DnsTxError::ZeroBondAmount,
        DnsTxError::InvalidPubKeyLen(3),
        DnsTxError::InvalidSignatureLen(4),
        DnsTxError::EmptyShard,
        DnsTxError::ShardTooLarge(99),
        DnsTxError::ShardTupleMismatch,
        DnsTxError::EvidenceTripleMismatch,
        DnsTxError::EvidenceNotIncompatible,
    ] {
        assert!(!e.to_string().is_empty());
    }
}

// ---- PR-10.9 foundation: stake-bond lifecycle helpers ----

#[test]
fn stake_bond_record_from_payload_initializes_pending() {
    let payload = fixture_bond(); // amount 100e9, activation 5_000, unbonding 100_000
    let op = fixture_outpoint();
    let rec = stake_bond_record_from_payload(&payload, op);
    assert_eq!(rec.bond_outpoint, op);
    assert_eq!(rec.version, payload.version);
    assert_eq!(rec.owner_pubkey_hash, payload.owner_pubkey_hash);
    assert_eq!(rec.validator_pubkey_hash, payload.validator_pubkey_hash);
    assert_eq!(rec.validator_pubkey, payload.validator_pubkey);
    assert_eq!(rec.amount, payload.amount);
    assert_eq!(rec.activation_daa_score, payload.activation_daa_score);
    assert_eq!(rec.unbonding_period_blocks, payload.unbonding_period_blocks);
    assert_eq!(rec.unbond_request_daa_score, None);
    assert_eq!(rec.slashed_at_daa_score, None);
    assert_eq!(rec.status, BondStatus::Pending);
}

#[test]
fn effective_bond_status_activation_transition() {
    let rec = stake_bond_record_from_payload(&fixture_bond(), fixture_outpoint()); // activation 5_000
    assert_eq!(effective_bond_status(&rec, 0), BondStatus::Pending);
    assert_eq!(effective_bond_status(&rec, 4_999), BondStatus::Pending);
    assert_eq!(effective_bond_status(&rec, 5_000), BondStatus::Active); // inclusive at activation
    assert_eq!(effective_bond_status(&rec, 1_000_000), BondStatus::Active);
    assert!(!is_bond_active_at(&rec, 4_999));
    assert!(is_bond_active_at(&rec, 5_000));
}

#[test]
fn effective_bond_status_unbonding_then_slashed_precedence() {
    let mut rec = stake_bond_record_from_payload(&fixture_bond(), fixture_outpoint()); // activation 5_000, unbonding_period 100_000
    // Active before any unbond/slash.
    assert_eq!(effective_bond_status(&rec, 10_000), BondStatus::Active);

    // Unbond requested at 20_000 -> Unbonding from that height (not active).
    rec.unbond_request_daa_score = Some(20_000);
    assert_eq!(effective_bond_status(&rec, 19_999), BondStatus::Active);
    assert_eq!(effective_bond_status(&rec, 20_000), BondStatus::Unbonding);
    assert!(!is_bond_active_at(&rec, 20_000));
    assert_eq!(bond_release_daa_score(&rec), Some(120_000)); // 20_000 + 100_000

    // A slash at 25_000 takes precedence over the unbond from its height on.
    rec.slashed_at_daa_score = Some(25_000);
    assert_eq!(effective_bond_status(&rec, 24_999), BondStatus::Unbonding);
    assert_eq!(effective_bond_status(&rec, 25_000), BondStatus::Slashed);
    assert_eq!(effective_bond_status(&rec, u64::MAX), BondStatus::Slashed);
}

#[test]
fn bond_release_daa_score_none_without_unbond_and_saturates() {
    let mut rec = stake_bond_record_from_payload(&fixture_bond(), fixture_outpoint());
    assert_eq!(bond_release_daa_score(&rec), None);
    // saturating_add: a pathological u64::MAX request height never wraps early.
    rec.unbond_request_daa_score = Some(u64::MAX);
    assert_eq!(bond_release_daa_score(&rec), Some(u64::MAX));
}

fn dns_overlay_tx(subnetwork_id: SubnetworkId, payload: Vec<u8>) -> Transaction {
    Transaction::new(0, vec![], vec![], 0, subnetwork_id, 0, payload)
}

#[test]
fn bond_mutations_extracts_insert_and_slash() {
    let bond_payload = fixture_bond();
    let bond_tx = dns_overlay_tx(SUBNETWORK_ID_STAKE_BOND, borsh::to_vec(&bond_payload).unwrap());
    let expected_outpoint = TransactionOutpoint::new(bond_tx.id(), 0); // A.1: output 0

    let evidence = fixture_evidence(); // references fixture_outpoint() as its bond
    let slash_tx = dns_overlay_tx(SUBNETWORK_ID_SLASHING_EVIDENCE, borsh::to_vec(&evidence).unwrap());

    // Attestation-shard + a native tx contribute no bond mutations.
    let shard_tx = dns_overlay_tx(SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, borsh::to_vec(&fixture_shard(2)).unwrap());
    let native_tx = dns_overlay_tx(SubnetworkId::from_byte(0), vec![1, 2, 3]);

    let muts = bond_mutations_from_accepted_txs(&[bond_tx, slash_tx, shard_tx, native_tx], 12_345, 0, 0, false);
    assert_eq!(muts.len(), 2);
    // P-1B: the inserted record's activation is clamped up to the acceptance DAA (12_345 here),
    // so a bond cannot back-date itself into a past epoch's active set / StakeScore denominator.
    let mut expected_record = stake_bond_record_from_payload(&bond_payload, expected_outpoint);
    expected_record.activation_daa_score = expected_record.activation_daa_score.max(12_345);
    // ADR-0022: `created_daa_score` is stamped with the acceptance DAA (the bond's creation point).
    expected_record.created_daa_score = 12_345;
    assert_eq!(muts[0], BondMutation::Insert(expected_outpoint, expected_record));
    assert_eq!(muts[1], BondMutation::Slash(evidence.bond_outpoint, 12_345));
}

#[test]
fn bond_activation_clamped_to_acceptance_daa() {
    // P-1B (DNS v2/v3): a bond declaring a past (or zero) activation cannot back-date itself
    // into a historical epoch's active set — the inserted record's activation is
    // max(declared, acceptance DAA). A future-dated activation is left as declared.
    let mut past = fixture_bond();
    past.activation_daa_score = 5;
    let past_tx = dns_overlay_tx(SUBNETWORK_ID_STAKE_BOND, borsh::to_vec(&past).unwrap());
    match &bond_mutations_from_accepted_txs(&[past_tx], 10_000, 0, 0, false)[0] {
        BondMutation::Insert(_, record) => {
            assert_eq!(record.activation_daa_score, 10_000, "past activation is clamped up to acceptance DAA");
        }
        _ => panic!("expected an Insert mutation"),
    }

    let mut future = fixture_bond();
    future.activation_daa_score = 50_000;
    let future_tx = dns_overlay_tx(SUBNETWORK_ID_STAKE_BOND, borsh::to_vec(&future).unwrap());
    match &bond_mutations_from_accepted_txs(&[future_tx], 10_000, 0, 0, false)[0] {
        BondMutation::Insert(_, record) => {
            assert_eq!(record.activation_daa_score, 50_000, "future activation is left as declared (no down-clamp)");
        }
        _ => panic!("expected an Insert mutation"),
    }
}

#[test]
fn shard_tuple_rejects_target_daa_score_mismatch() {
    // P-1C (DNS v2/v3): the shard's (epoch, target_hash, target_daa_score, vsc) tuple must
    // match every contained attestation — including target_daa_score (previously unchecked).
    let att = fixture_attestation();
    let mut shard = single_attestation_shard(att);
    // Desync ONLY target_daa_score (the attestation keeps its own value).
    shard.target_daa_score = shard.target_daa_score.wrapping_add(1);
    let payload = borsh::to_vec(&shard).unwrap();
    assert!(
        matches!(validate_stake_attestation_shard_payload(&payload), Err(DnsTxError::ShardTupleMismatch)),
        "a shard whose target_daa_score disagrees with its attestation must be rejected"
    );
}

// ---- DNS v3: Canonical Lagged Anchor (blue_score-coordinated) ----

#[test]
fn ready_epoch_from_tip_blue_score_off_by_one() {
    // L=100, lag=20 -> epoch E is ready iff tip_blue >= epoch_end(E)+lag = (E+1)*100-1+20.
    assert_eq!(ready_epoch_from_tip_blue_score(118, 100, 20), None); // epoch_end(0)+lag = 119
    assert_eq!(ready_epoch_from_tip_blue_score(119, 100, 20), Some(0));
    assert_eq!(ready_epoch_from_tip_blue_score(219, 100, 20), Some(1));
    assert_eq!(ready_epoch_from_tip_blue_score(0, 100, 20), None);
}

#[test]
fn canonical_anchor_most_recent_at_or_below_cutoff() {
    // L=100, backoff=10 -> cutoff(1) = epoch_end(1)-10 = 199-10 = 189.
    let h = |n: u8| Hash64::from_bytes([n; 64]);
    let ancestors = vec![(h(9), 250, 2500), (h(8), 190, 1900), (h(7), 185, 1850), (h(6), 100, 1000), (h(5), 50, 500)];
    let a = canonical_lagged_epoch_anchor(1, 100, 10, &ancestors).unwrap();
    assert_eq!(a.anchor_hash, h(7), "most-recent ancestor with blue_score <= 189 is h7@185");
    assert_eq!(a.anchor_blue_score, 185);
    assert_eq!(a.anchor_daa_score, 1850);
    assert_eq!(a.cutoff_blue_score, 189);
    assert_eq!(a.epoch_end_blue_score, 199);
    assert!(!a.duplicate_of_previous_anchor);
}

#[test]
fn canonical_anchor_no_hole_on_blue_score_jump() {
    // High-parallel: blue_score jumps 300 -> 80 across one selected-chain step, so NO ancestor
    // lands inside epoch 1's [100,199] band — yet the anchor still exists (most-recent at/below
    // the cutoff). The "in-epoch block" requirement is what created the original hole; the
    // cutoff (<=) formulation removes it without falling back to the (split-prone) chain index.
    let h = |n: u8| Hash64::from_bytes([n; 64]);
    let ancestors = vec![(h(9), 300, 3000), (h(8), 80, 800), (h(7), 10, 100)];
    let a = canonical_lagged_epoch_anchor(1, 100, 10, &ancestors).unwrap(); // cutoff = 189
    assert_eq!(a.anchor_hash, h(8), "anchor exists even with no in-epoch block");
    assert_eq!(a.anchor_blue_score, 80);
}

#[test]
fn canonical_anchor_duplicate_flagged_and_pruning_base_invariant() {
    // The anchor depends ONLY on (hash, blue_score) — there is no index input — so the
    // selected-chain index ORIGIN (archival genesis=0 vs IBD pruning_point=0) is irrelevant:
    // identical blue_score data => identical anchor. That is the pruning-base invariance that
    // the retracted chain-index design lacked.
    let h = |n: u8| Hash64::from_bytes([n; 64]);
    // Sparse chain: nothing in (189,289] or (289,389], so epochs 2 and 3 reuse anchors.
    let ancestors = vec![(h(9), 400, 4000), (h(8), 250, 2500), (h(7), 150, 1500), (h(6), 50, 500)];
    let e1 = canonical_lagged_epoch_anchor(1, 100, 10, &ancestors).unwrap(); // cutoff 189 -> h7@150
    let e2 = canonical_lagged_epoch_anchor(2, 100, 10, &ancestors).unwrap(); // cutoff 289 -> h8@250
    let e3 = canonical_lagged_epoch_anchor(3, 100, 10, &ancestors).unwrap(); // cutoff 389 -> h8@250
    assert_eq!(e1.anchor_hash, h(7));
    assert!(!e1.duplicate_of_previous_anchor);
    assert_eq!(e2.anchor_hash, h(8));
    assert!(!e2.duplicate_of_previous_anchor, "e2 anchor (h8) differs from e1 anchor (h7)");
    assert_eq!(e3.anchor_hash, h(8));
    assert!(e3.duplicate_of_previous_anchor, "epoch 3 reuses epoch 2's anchor -> not creditable");
}

#[test]
fn dns_v3_params_consistency_gate() {
    use crate::config::params::{GENESIS_ACTIVE_DNS_PARAMS, PRODUCTION_DNS_PARAMS};
    // The SHIPPED presets must be v3-consistent — else the reorg gate could never enter
    // Active on the very networks that use them.
    assert!(GENESIS_ACTIVE_DNS_PARAMS.dns_v3_params_consistent(), "devnet/simnet preset is v3-consistent");
    assert!(PRODUCTION_DNS_PARAMS.dns_v3_params_consistent(), "mainnet/testnet preset is v3-consistent");

    // Each invariant violation flips the gate false (fail-safe: update_dns_state then never
    // enters Active, so finality stays dormant rather than ill-defined).
    let mut p = GENESIS_ACTIVE_DNS_PARAMS;
    p.attestation_epoch_length_blue_score = 0;
    assert!(!p.dns_v3_params_consistent(), "a zero epoch length is rejected");

    let mut p = GENESIS_ACTIVE_DNS_PARAMS;
    p.attestation_lag_blue_score = 0;
    assert!(!p.dns_v3_params_consistent(), "a zero lag is rejected");

    let mut p = GENESIS_ACTIVE_DNS_PARAMS;
    p.attestation_anchor_backoff_blue_score = p.attestation_epoch_length_blue_score;
    assert!(!p.dns_v3_params_consistent(), "backoff >= epoch length is rejected");

    let mut p = GENESIS_ACTIVE_DNS_PARAMS;
    p.stake_score_window_blue_score = p.attestation_epoch_length_blue_score; // < depth·L + lag + backoff
    assert!(!p.dns_v3_params_consistent(), "a window too short to cover the creditable horizon is rejected");
    // audit M-05: covering only 2L (not required_stake_depth epochs) must be rejected.
    let mut p = GENESIS_ACTIVE_DNS_PARAMS;
    let depth_epochs = (p.required_stake_depth.0 / STAKE_SCORE_SCALE) as u64;
    assert!(depth_epochs > 2, "fixture assumes a deep stake horizon");
    p.stake_score_window_blue_score =
        2 * p.attestation_epoch_length_blue_score + p.attestation_lag_blue_score + p.attestation_anchor_backoff_blue_score;
    assert!(!p.dns_v3_params_consistent(), "covering only 2L is insufficient when required_stake_depth > 2 epochs");
}

#[test]
fn shipped_dns_presets_keep_attestation_out_of_base_chain_validity() {
    use crate::config::params::{GENESIS_ACTIVE_DNS_PARAMS, PRODUCTION_DNS_PARAMS, TESTNET_DNS_PARAMS};

    for (name, params) in [
        ("genesis-active/dev-sim", GENESIS_ACTIVE_DNS_PARAMS),
        ("production/mainnet", PRODUCTION_DNS_PARAMS),
        ("testnet", TESTNET_DNS_PARAMS),
    ] {
        assert_eq!(
            params.mandatory_attestation_inclusion_daa_score,
            u64::MAX,
            "{name} must be liveness-first: missing attestations degrade finality, not block validity"
        );
        assert_eq!(params.stake_event_quality_floor_bps, 6000, "{name} must retain the 60% StakeScore/reward quality floor");
    }
}

#[test]
fn bridge_finality_freshness_requires_confirmed_non_stale_dns_anchor() {
    let anchor = Hash64::from_bytes([0x42; 64]);
    let anchor_blue = 10_000;
    let dns = crate::config::params::PRODUCTION_DNS_PARAMS;
    let bound = dns_bridge_max_anchor_distance_blue_score(&dns);
    assert_eq!(bound, dns_anchor_healthy_distance_blue_score(&dns) + DEFAULT_BRIDGE_FINALITY_MAX_STALENESS_DAA_SCORE);

    assert!(dns_finality_fresh_for_bridge(true, anchor, Some(anchor_blue), anchor_blue + bound, &dns));
    assert!(!dns_finality_fresh_for_bridge(true, anchor, Some(anchor_blue), anchor_blue + bound + 1, &dns), "a stall pauses");
    assert!(!dns_finality_fresh_for_bridge(false, anchor, Some(anchor_blue), anchor_blue, &dns), "unconfirmed pauses");
    assert!(!dns_finality_fresh_for_bridge(true, Hash64::default(), Some(0), 0, &dns), "no anchor pauses");
    assert!(!dns_finality_fresh_for_bridge(true, anchor, Some(anchor_blue), anchor_blue - 1, &dns), "an anchor above the tip pauses");
    assert!(!dns_finality_fresh_for_bridge(true, anchor, None, anchor_blue, &dns), "an anchor this node cannot read pauses");
}

/// **testnet-11's bridge could not open, and now opens exactly while confirmation keeps up.**
///
/// The 120 s preset tolerates 2 of staleness, and the old predicate read it as a DAA distance
/// that had to CONTAIN the confirmed anchor's distance below the tip. That distance is never
/// below lag + backoff = 3 blue, and DAA runs ahead of blue score: on 2026-09-10 DNS finality was
/// confirmed on testnet-11 at anchor DAA 3149 / blue 2188 with the sink at DAA 3161 / blue 2197,
/// and every deposit claim came back `EVM bridge is paused`. Pinned from both sides: the healthy
/// distances measured while confirmation was keeping up (5–12 blue) are fresh, and a stall
/// pauses the bridge within the preset's 2 blue of tolerance past the healthy distance.
#[test]
fn testnet_11_opens_the_bridge_while_dns_confirmation_keeps_up_and_pauses_on_a_stall() {
    let t11 = crate::config::params::Params::from(crate::network::NetworkId::with_suffix(crate::network::NetworkType::Testnet, 11))
        .dns_params
        .expect("testnet-11 carries the DNS overlay");
    assert_eq!(
        (
            t11.attestation_lag_blue_score,
            t11.attestation_anchor_backoff_blue_score,
            t11.attestation_epoch_length_blue_score,
            t11.bridge_finality_max_staleness_daa_score
        ),
        (2, 1, 2, 2),
        "the preset this test reasons about"
    );
    assert_eq!(dns_anchor_healthy_distance_blue_score(&t11), 12);
    assert_eq!(dns_bridge_max_anchor_distance_blue_score(&t11), 14);

    let anchor = Hash64::from_bytes([0x31; 64]);
    // The measured confirmation: blue 2188 under a sink at blue 2197 — fresh now. (The old DAA
    // reading saw 3161 − 3149 = 12 > 2 and paused.)
    assert!(dns_finality_fresh_for_bridge(true, anchor, Some(2188), 2197, &t11));
    // Every healthy distance measured on the live chain.
    for distance in 5..=12u64 {
        assert!(dns_finality_fresh_for_bridge(true, anchor, Some(2188), 2188 + distance, &t11), "healthy {distance}");
    }
    // A stall: the anchor stops while the tip climbs. Open through the tolerance, paused after.
    assert!(dns_finality_fresh_for_bridge(true, anchor, Some(2188), 2188 + 14, &t11));
    assert!(!dns_finality_fresh_for_bridge(true, anchor, Some(2188), 2188 + 15, &t11), "stalled: paused");
    // And it never opens on an unconfirmed state, whatever the distance.
    assert!(!dns_finality_fresh_for_bridge(false, anchor, Some(2188), 2197, &t11));
}

#[test]
fn bond_mutations_skips_undecodable_overlay_payload() {
    // A malformed stake-bond payload is defensively skipped, not panicked on.
    let bad = dns_overlay_tx(SUBNETWORK_ID_STAKE_BOND, vec![0xff, 0x00, 0x12]);
    assert!(bond_mutations_from_accepted_txs(&[bad], 0, 0, 0, false).is_empty());
}

#[test]
fn bond_mutations_empty_without_overlay_txs() {
    let native = dns_overlay_tx(SubnetworkId::from_byte(0), vec![]);
    let coinbase = dns_overlay_tx(SubnetworkId::from_byte(1), vec![]);
    assert!(bond_mutations_from_accepted_txs(&[native, coinbase], 100, 0, 0, false).is_empty());
}

#[test]
fn bond_mutations_enforce_min_amount_and_unbonding_floor() {
    // fixture_bond: amount = 100_000_000_000, declared unbonding_period_blocks = 100_000.
    let bond_tx = dns_overlay_tx(SUBNETWORK_ID_STAKE_BOND, borsh::to_vec(&fixture_bond()).unwrap());

    // (a) below the per-bond minimum → not admitted (no mutation).
    assert!(
        bond_mutations_from_accepted_txs(std::slice::from_ref(&bond_tx), 1, 100_000_000_001, 0, false).is_empty(),
        "a bond below min_bond_amount must be rejected"
    );
    // (b) at/above the minimum → admitted.
    let muts = bond_mutations_from_accepted_txs(std::slice::from_ref(&bond_tx), 1, 100_000_000_000, 0, false);
    assert_eq!(muts.len(), 1, "a bond at exactly the minimum is accepted");

    // (c) the stored unbonding period is clamped UP to the floor (declared 100_000 < 500_000).
    let muts = bond_mutations_from_accepted_txs(std::slice::from_ref(&bond_tx), 1, 0, 500_000, false);
    match &muts[0] {
        BondMutation::Insert(_, rec) => assert_eq!(rec.unbonding_period_blocks, 500_000, "unbonding clamped to floor"),
        other => panic!("expected Insert, got {other:?}"),
    }
    // (d) a floor below the declared value leaves the larger declared value intact.
    let muts = bond_mutations_from_accepted_txs(&[bond_tx], 1, 0, 50_000, false);
    match &muts[0] {
        BondMutation::Insert(_, rec) => assert_eq!(rec.unbonding_period_blocks, 100_000, "declared > floor is kept"),
        other => panic!("expected Insert, got {other:?}"),
    }
}

// ---- Addendum B §B.1: ActiveBondView + RewardedEpochSet ----

fn fixture_bond_record(op: TransactionOutpoint) -> StakeBondRecord {
    // activation_daa_score = 5_000, status = Pending.
    stake_bond_record_from_payload(&fixture_bond(), op)
}

// ----- ADR-0018 "本格版" (PoS-v2) Phase 1: epoch accumulator pure core -----

fn op(byte: u8) -> TransactionOutpoint {
    TransactionOutpoint::new(Hash64::from_bytes([byte; 64]), 0)
}

/// A bond with controllable amount / activation / reward payload (for accumulator tests).
fn bond_rec(op: TransactionOutpoint, amount: u64, activation: u64, payload_byte: u8) -> StakeBondRecord {
    let mut r = fixture_bond_record(op);
    r.amount = amount;
    r.activation_daa_score = activation;
    r.owner_reward_spk_payload = [payload_byte; 64];
    r
}

fn contrib(daa: u64, keys: &[(TransactionOutpoint, u64)], quality: u64) -> BlockEpochContribution {
    BlockEpochContribution { block_daa_score: daa, rewarded_keys: keys.to_vec(), quality_subpool: quality }
}

/// Accumulation: `included` is keyed by **attestation** epoch (resolved against the bond
/// snapshot, in contribution order) while `quality_pool_accrued` is keyed by **block** epoch
/// — the two attributions are independent. Each block also makes its own epoch present.
#[test]
fn recompute_epoch_tallies_accumulates_included_and_quality() {
    let (a, b) = (op(0xA1), op(0xB2));
    let bonds = vec![bond_rec(a, 100, 0, 0xA1), bond_rec(b, 200, 0, 0xB2)];
    // Two blocks in epoch 1 reward epoch-0 attestations (recency lag); no block in epoch 0.
    let contributions = vec![contrib(12, &[(a, 0)], 5), contrib(15, &[(b, 0)], 7)];

    let tallies = recompute_epoch_tallies(30, 10, 100, &contributions, &bonds);
    assert_eq!(
        tallies,
        vec![
            // epoch 0: included = both attesters (contribution order), no block of its own → pool 0.
            (
                0,
                EpochTally {
                    expected_stake: 300,
                    included: vec![(Hash64::from_bytes([0xA1; 64]), 100), (Hash64::from_bytes([0xB2; 64]), 200)],
                    quality_pool_accrued: 0,
                    finalized: false,
                }
            ),
            // epoch 1: the two blocks live here → pool 5+7, but no epoch-1 attestation was rewarded.
            (1, EpochTally { expected_stake: 300, included: vec![], quality_pool_accrued: 12, finalized: false }),
        ]
    );
}

/// `expected_stake` is evaluated at each epoch's anchor (`epoch · epoch_length`): a bond that
/// only activates later is excluded from earlier epochs' denominators.
#[test]
fn recompute_epoch_tallies_expected_stake_respects_activation() {
    let (a, b) = (op(0xA1), op(0xB2));
    // b activates at daa 100 (epoch 10) — absent from epoch 0/1 denominators.
    let bonds = vec![bond_rec(a, 100, 0, 0xA1), bond_rec(b, 200, 100, 0xB2)];
    let contributions = vec![contrib(5, &[(a, 0)], 0), contrib(15, &[(a, 1)], 0)];
    let tallies = recompute_epoch_tallies(40, 10, 100, &contributions, &bonds);
    // epoch 0 anchor 0, epoch 1 anchor 10 — both before b's activation → expected 100.
    assert_eq!(tallies[0].1.expected_stake, 100);
    assert_eq!(tallies[1].1.expected_stake, 100);
}

/// Finalization is one-way at `sink_daa ≥ (E+1)·epoch_length + finalization_depth`.
#[test]
fn recompute_epoch_tallies_finalization_boundary() {
    let a = op(0xA1);
    let bonds = vec![bond_rec(a, 100, 0, 0xA1)];
    let contributions = vec![contrib(5, &[], 3)]; // one block in epoch 0
    // epoch 0 finalizes at (0+1)*10 + 100 = 110.
    assert!(!recompute_epoch_tallies(109, 10, 100, &contributions, &bonds)[0].1.finalized);
    assert!(recompute_epoch_tallies(110, 10, 100, &contributions, &bonds)[0].1.finalized);
}

/// No contributions ⇒ no tallies.
#[test]
fn recompute_epoch_tallies_empty() {
    assert!(recompute_epoch_tallies(1_000, 10, 100, &[], &[bond_rec(op(0xA1), 100, 0, 0xA1)]).is_empty());
}

/// A rewarded key whose bond is absent from the snapshot is skipped from `included` (the epoch
/// is still emitted because the block lives in it).
#[test]
fn recompute_epoch_tallies_unresolvable_bond_skipped() {
    let (a, ghost) = (op(0xA1), op(0xEE));
    let bonds = vec![bond_rec(a, 100, 0, 0xA1)];
    let contributions = vec![contrib(3, &[(ghost, 0)], 4)]; // block in epoch 0 rewards an unknown bond
    let tallies = recompute_epoch_tallies(30, 10, 100, &contributions, &bonds);
    assert_eq!(tallies.len(), 1);
    assert_eq!(tallies[0].0, 0);
    assert!(tallies[0].1.included.is_empty());
    assert_eq!(tallies[0].1.quality_pool_accrued, 4);
}

/// One block may reward attestations for several distinct epochs; each lands in its own tally.
#[test]
fn recompute_epoch_tallies_multi_epoch_block() {
    let (a, b) = (op(0xA1), op(0xB2));
    let bonds = vec![bond_rec(a, 100, 0, 0xA1), bond_rec(b, 200, 0, 0xB2)];
    let contributions = vec![contrib(25, &[(a, 0), (b, 1)], 9)]; // block in epoch 2
    let tallies = recompute_epoch_tallies(30, 10, 100, &contributions, &bonds);
    assert_eq!(tallies.len(), 3);
    assert_eq!(
        tallies[0],
        (
            0,
            EpochTally {
                expected_stake: 300,
                included: vec![(Hash64::from_bytes([0xA1; 64]), 100)],
                quality_pool_accrued: 0,
                finalized: false
            }
        )
    );
    assert_eq!(
        tallies[1],
        (
            1,
            EpochTally {
                expected_stake: 300,
                included: vec![(Hash64::from_bytes([0xB2; 64]), 200)],
                quality_pool_accrued: 0,
                finalized: false
            }
        )
    );
    assert_eq!(tallies[2], (2, EpochTally { expected_stake: 300, included: vec![], quality_pool_accrued: 9, finalized: false }));
}

/// Reorg-safety: the tally is a pure function of the supplied (selected-chain) contributions, so
/// re-deriving an epoch from a different chain's blocks yields that chain's tally — no stale state.
#[test]
fn recompute_epoch_tallies_is_pure_per_chain() {
    let (a, b) = (op(0xA1), op(0xB2));
    let bonds = vec![bond_rec(a, 100, 0, 0xA1), bond_rec(b, 200, 0, 0xB2)];
    let chain_a = recompute_epoch_tallies(30, 10, 100, &[contrib(2, &[(a, 0)], 5)], &bonds);
    let chain_b = recompute_epoch_tallies(30, 10, 100, &[contrib(2, &[(b, 0)], 8)], &bonds);
    assert_eq!(chain_a[0].1.included, vec![(Hash64::from_bytes([0xA1; 64]), 100)]);
    assert_eq!(chain_a[0].1.quality_pool_accrued, 5);
    assert_eq!(chain_b[0].1.included, vec![(Hash64::from_bytes([0xB2; 64]), 200)]);
    assert_eq!(chain_b[0].1.quality_pool_accrued, 8);
}

// ----- ADR-0018 "本格版" (PoS-v2) Phase 2: φS gate + deferred quality-bonus outputs -----

/// The φS gate is the integer cross-multiply `included·10_000 ≥ φS_bps·expected`, with the
/// `expected == 0` and `φS_bps == 0` edge cases both meeting vacuously.
#[test]
fn epoch_meets_quality_floor_boundary() {
    // φS = 60%: included must be ≥ 600/1000.
    assert!(epoch_meets_quality_floor(600, 1000, 6000)); // exactly at the floor → meets
    assert!(!epoch_meets_quality_floor(599, 1000, 6000)); // one below → misses
    assert!(epoch_meets_quality_floor(601, 1000, 6000));
    // φS = 0 → always meets (pre-ADR-0018 behavior).
    assert!(epoch_meets_quality_floor(0, 1000, 0));
    // expected = 0 → vacuously meets.
    assert!(epoch_meets_quality_floor(0, 0, 6000));
}

/// Met epoch: each included validator is paid a proportional share of the quality pool, in
/// stored order, to its ML-DSA P2PKH script.
#[test]
fn validator_quality_bonus_outputs_met_pays_proportional() {
    let included = vec![(Hash64::from_bytes([0xA1; 64]), 100u64), (Hash64::from_bytes([0xB2; 64]), 300u64)];
    let out = validator_quality_bonus_outputs(1000, &included, 400, true, Some(16));
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].value, 250); // 1000 · 100/400
    assert_eq!(out[0].script_public_key, p2pkh_mldsa87_spk(&[0xA1; 64]));
    assert_eq!(out[1].value, 750); // 1000 · 300/400
    assert_eq!(out[1].script_public_key, p2pkh_mldsa87_spk(&[0xB2; 64]));
    // Value-conserving: Σ ≤ pool.
    assert!(out.iter().map(|o| o.value as u128).sum::<u128>() <= 1000);
}

/// Unmet epoch ⇒ no outputs (the whole pool rolls over).
#[test]
fn validator_quality_bonus_outputs_unmet_is_empty() {
    let included = vec![(Hash64::from_bytes([0xA1; 64]), 100u64)];
    assert!(validator_quality_bonus_outputs(1000, &included, 400, false, Some(16)).is_empty());
}

/// A zero-stake (or zero-share) validator emits no output even when the epoch met φS.
#[test]
fn validator_quality_bonus_outputs_zero_share_skipped() {
    let included = vec![(Hash64::from_bytes([0xA1; 64]), 0u64), (Hash64::from_bytes([0xB2; 64]), 400u64)];
    let out = validator_quality_bonus_outputs(1000, &included, 400, true, Some(16));
    assert_eq!(out.len(), 1); // only the 0xB2 validator
    assert_eq!(out[0].value, 1000);
    assert_eq!(out[0].script_public_key, p2pkh_mldsa87_spk(&[0xB2; 64]));
}

/// **§E's stated precondition, as a test rather than as a sentence** (mainnet audit,
/// 2026-09-06, H-2).
///
/// `validator_quality_bonus_outputs`' doc claimed "the outputs sum to ≤ `quality_pool` (Σ
/// included stake ≤ `expected_stake`)". `recompute_epoch_tallies` fills `included` from the
/// ATTESTATION epoch and `expected_stake` from a DAA-denominated anchor, so the parenthesis is
/// not a fact. The fixture directly above cannot see it: it uses included 100 + 300 against
/// `expected_stake` 400 and then asserts `Σ ≤ 1000`, which holds for every pool and every
/// implementation, including one that mints twice the pool on an incoherent epoch.
///
/// The RULE, not the patch: an epoch whose numerator is not drawn from its own denominator is
/// not priceable, and under §E an unpriceable pool ROLLS OVER rather than minting more than it
/// holds. The pre-fence arm is pinned as a NUMBER in the same test so testnet-11's behaviour
/// can neither drift nor be quietly "fixed" under it — closing it there is a flag day.
#[test]
fn the_quality_bonus_refuses_an_epoch_whose_included_set_outruns_its_denominator() {
    let included = vec![(Hash64::from_bytes([0xA1; 64]), 400u64), (Hash64::from_bytes([0xB2; 64]), 400u64)];
    let unfenced = validator_quality_bonus_outputs(1000, &included, 400, true, None);
    assert_eq!(
        unfenced.iter().map(|o| o.value as u128).sum::<u128>(),
        2000,
        "below `palw_validator_payout_bounds` this mints twice the pool it was handed — the H-2 mint, \
             pinned so it cannot widen or close in silence"
    );
    let bounded = validator_quality_bonus_outputs(1000, &included, 400, true, Some(16));
    assert!(bounded.is_empty(), "past the fence an epoch whose Σ included stake exceeds its expected stake pays nothing");
}

/// The count bound the coinbase's isolation rule reserves for this fan-out, with the pool cap's
/// tail semantics: the canonical-order tail is dropped and NOT minted (§E rollover). The
/// pre-fence arm is pinned in the same test — one output per included validator, unbounded.
#[test]
fn the_quality_bonus_never_pays_more_outputs_than_the_block_reserved() {
    let included: Vec<(Hash64, u64)> = (0..40u8).map(|i| (Hash64::from_bytes([i; 64]), 10u64)).collect();
    let bounded = validator_quality_bonus_outputs(4000, &included, 400, true, Some(16));
    assert_eq!(bounded.len(), 16, "the block's remaining slot budget bounds the fan-out");
    assert!(bounded.iter().map(|o| o.value as u128).sum::<u128>() <= 4000);
    assert_eq!(validator_quality_bonus_outputs(4000, &included, 400, true, None).len(), 40);
}

/// **One map, two numberings — stated as a fact, because closing it is a design change.**
///
/// `recompute_epoch_tallies` keys `quality_pool_accrued` and `expected_stake` by
/// `block_daa_score / epoch_length_blocks` and `included` by the attestation epoch, which is
/// blue-score denominated. On a chain with reds the two indices differ by `(daa − blue)/L`, and
/// once that exceeds `walk_bound / L` they stop intersecting at all. Reconciling them needs the
/// blue score of every window block including the below-pruning-point ones, which live in
/// `OverlaySnapshot` — committed in `Header::overlay_commitment_root` — so it is not repairable
/// without moving testnet-11's block validity. That is the user's decision, not this test's.
///
/// This test states the gap and the ONE property that must hold across it: a tally built from
/// two clocks has an `included` set not drawn from its own `expected_stake`, and past the fence
/// it pays NOTHING rather than minting. If the numberings are ever reconciled the first
/// assertion fails, which is the signal that the gap closed — it must not close in silence.
#[test]
fn the_accumulator_keys_one_map_from_two_epoch_numberings() {
    let (a, b) = (op(0xA1), op(0xB2));
    // `b` activates at DAA 25 — AFTER epoch 2's DAA-denominated anchor (2 · 10 = 20), so it is
    // absent from `expected_stake`. The block at DAA 28 lives in block-epoch 2 and rewards an
    // attestation whose ATTESTATION epoch is 2 for both bonds, so both land in `included`.
    // That is the two-clock drift in miniature: the numerator carries a bond the denominator
    // does not.
    let bonds = vec![bond_rec(a, 100, 0, 0xA1), bond_rec(b, 200, 25, 0xB2)];
    let contributions = vec![contrib(28, &[(a, 2), (b, 2)], 1000)];
    let tallies = recompute_epoch_tallies(1_000, 10, 0, &contributions, &bonds);
    let (_, tally) = tallies.iter().find(|(e, _)| *e == 2).expect("epoch 2 is touched by both numberings");
    let included_sum: u128 = tally.included.iter().map(|(_, s)| *s as u128).sum();
    assert_eq!(tally.expected_stake, 100, "the denominator is evaluated at the DAA anchor, where only `a` is active");
    assert!(
        included_sum > tally.expected_stake,
        "the two numberings still disagree: `included` ({included_sum}) carries a bond `expected_stake` \
             ({}) does not",
        tally.expected_stake
    );

    // Below the fence such an epoch mints more than its pool — pinned as the number.
    let unfenced = validator_quality_bonus_outputs(tally.quality_pool_accrued, &tally.included, tally.expected_stake, true, None);
    assert_eq!(
        unfenced.iter().map(|o| o.value as u128).sum::<u128>(),
        2 * tally.quality_pool_accrued,
        "testnet-11's arm pays each of the two a full pool"
    );
    assert!(
        validator_quality_bonus_outputs(tally.quality_pool_accrued, &tally.included, tally.expected_stake, true, Some(16)).is_empty(),
        "and past the fence such an epoch pays nothing instead of minting"
    );
}

/// A finalized-epoch tally with `n` equal-stake included validators, for the per-block payout
/// tests below. `expected_stake` is the true sum, so each epoch is individually coherent — the
/// defect under test is the BLOCK's budget, not the epoch's.
#[cfg(test)]
fn tally_of(n: u8, stake: u64, pool: u128) -> EpochTally {
    EpochTally {
        expected_stake: (n as u128) * (stake as u128),
        included: (0..n).map(|i| (Hash64::from_bytes([i; 64]), stake)).collect(),
        quality_pool_accrued: pool,
        finalized: true,
    }
}

/// **RFC-0012 §4.4 (lead default 2026-10-16, pending user review): past `palw_dns_retirement_v1` the drip and the
/// deferred bonus pay only pre-retirement entitlements.** A retired block stores no rewarded keys
/// (`utxo_validation.rs:1394-1396`) and no participation enters past the fence (`:1091-1095`), so an epoch whose blocks
/// all lie past the fence reaches these loops with an EMPTY included set (and, the validator pool being 0 past the fence,
/// `processor.rs:4030-4040`, no quality pool either). Both loops then pay nothing for it and the reserve is untouched, while a
/// pre-retirement epoch crossed in the same block is still paid from the existing reserve only. Source only, not executed.
#[test]
fn a_post_retirement_epoch_without_an_included_set_is_paid_nothing_by_the_drip_or_the_bonus() {
    let cap = 1_000u64;
    let reserve = 1_000_000u64;
    let post_retirement = EpochTally { expected_stake: 100, included: Vec::new(), quality_pool_accrued: 0, finalized: true };
    let only_post = vec![(7_000u64, post_retirement.clone())];
    for bounds in [None, Some(PALW_V2_MAX_RESERVE_DRIP_PAYOUTS)] {
        let (outputs, total) = reserve_drip_outputs_for_block(reserve, cap, bounds, &only_post);
        assert!(outputs.is_empty() && total == 0, "no included set, no drip, no reserve movement ({bounds:?})");
        assert!(deferred_quality_bonus_outputs_for_block(0, bounds, &only_post).is_empty());
    }
    // Mixed crossing: the pre-retirement epoch is paid once per included validator, from the reserve, within the cap.
    let mixed = vec![(6_999u64, tally_of(2, 50, 0)), (7_000u64, post_retirement)];
    let (outputs, total) = reserve_drip_outputs_for_block(reserve, cap, Some(PALW_V2_MAX_RESERVE_DRIP_PAYOUTS), &mixed);
    assert_eq!(outputs.len(), 2, "one drip per included validator of the pre-retirement epoch only");
    assert!(total <= cap && total <= reserve, "paid from the existing reserve, within the per-block cap");
}

/// **The reserve drip's rate limit is spent per BLOCK, not per crossing** (mainnet audit,
/// 2026-09-06, M-1).
///
/// `reserve_drip_per_epoch_cap_sompi` is documented as a rate limiter on the security reserve.
/// A rate must not depend on how many epoch thresholds one wide mergeset happens to cross, and
/// `at_two_minute_cadence` set `epoch_length_blocks = 2` without moving the number, so a block
/// that merges 180 crosses ~90 thresholds. `pos_v2_reserve_drip_pays_finalized_epoch` cannot
/// see this: it sets the cap to `u64::MAX` and its chain crosses one threshold per block, so
/// the multiplier has nothing to multiply.
///
/// The RULE is stated over the input a real block hands the loop — a LIST of finalized epochs.
/// The pre-fence arm is pinned as a NUMBER (three crossings release three caps) because that is
/// what testnet-11 runs and closing it there is a flag day, not a diff.
#[test]
fn the_reserve_drip_rate_limit_is_spent_per_block_not_per_crossing() {
    let cap = 1_000u64;
    // Three epochs finalized by ONE block, each with two included validators.
    let finalized: Vec<(u64, EpochTally)> = (0..3u64).map(|e| (e, tally_of(2, 50, 0))).collect();
    let reserve = 1_000_000u64;

    let (unfenced, unfenced_total) = reserve_drip_outputs_for_block(reserve, cap, None, &finalized);
    assert_eq!(
        unfenced_total,
        3 * cap,
        "below `palw_validator_payout_bounds` the cap is spent once per CROSSED EPOCH — three crossings, three caps; \
             the M-1 drain, pinned so it cannot widen or close in silence"
    );
    assert_eq!(unfenced.len(), 6, "…one output per included validator per crossed epoch");

    let (bounded, bounded_total) = reserve_drip_outputs_for_block(reserve, cap, Some(PALW_V2_MAX_RESERVE_DRIP_PAYOUTS), &finalized);
    assert!(
        bounded_total <= cap,
        "past the fence one block releases at most one cap however many thresholds it crossed ({bounded_total} > {cap})"
    );
    assert!(
        bounded.iter().map(|o| o.value).sum::<u64>() == bounded_total,
        "the minted total and the outputs are one statement — the reserve recurrence subtracts exactly what was paid"
    );

    // And the premise holds in the other direction: with ONE crossing the two arms agree, so
    // the fence changes nothing on the shape the parameter was written for.
    let one = &finalized[..1];
    assert_eq!(
        reserve_drip_outputs_for_block(reserve, cap, None, one).1,
        reserve_drip_outputs_for_block(reserve, cap, Some(PALW_V2_MAX_RESERVE_DRIP_PAYOUTS), one).1,
        "a steady chain crossing one threshold a block is byte-identical across the fence"
    );
}

/// **The deferred bonus's slot budget is the BLOCK's, not the epoch's** (mainnet audit,
/// 2026-09-06, H-3).
///
/// The fan-out is (epochs this block finalizes) × (validators included in each), so a per-epoch
/// bound bounds nothing: the isolation cap the node then applies to its own coinbase is a
/// per-BLOCK number. Stated over a multi-epoch crossing, which is the only input the product is
/// visible in, and with the pre-fence count pinned as the number testnet-11 builds.
#[test]
fn the_deferred_bonus_slot_budget_is_carried_across_the_blocks_epochs() {
    // Four epochs, twelve included validators each — mainnet's own `min_active_validators`.
    let finalized: Vec<(u64, EpochTally)> = (0..4u64).map(|e| (e, tally_of(12, 100, 1_200))).collect();

    let unfenced = deferred_quality_bonus_outputs_for_block(0, None, &finalized);
    assert_eq!(
        unfenced.len(),
        48,
        "below the fence one coinbase carries (epochs × included) outputs — the H-3 fan-out, pinned as a number"
    );

    let bounded = deferred_quality_bonus_outputs_for_block(0, Some(PALW_V2_MAX_DEFERRED_VALIDATOR_PAYOUTS), &finalized);
    assert_eq!(
        bounded.len() as u64,
        PALW_V2_MAX_DEFERRED_VALIDATOR_PAYOUTS,
        "past the fence the block's slot budget bounds the whole loop, not each epoch"
    );
    // The tail is dropped, not minted: the first epoch is paid in full and the rest roll over.
    assert!(bounded.iter().map(|o| o.value as u128).sum::<u128>() <= 4 * 1_200, "and nothing outside the epochs' own pools is minted");
}

/// The deferred-payout once-per-epoch guard: a block finalizes exactly the epochs whose
/// threshold `(E+1)·L + fd` is crossed in `(parent_daa, daa_score]`. With L=10, fd=100 the
/// thresholds are 110 (E0), 120 (E1), 130 (E2), …
#[test]
fn epochs_finalized_at_crossing() {
    // Nothing buried yet (daa below epoch-0's threshold 110).
    assert_eq!(epochs_finalized_at(108, 109, 10, 100), None);
    assert_eq!(epochs_finalized_at(0, 50, 10, 100), None); // daa < fd
    // Exactly epoch 0 crosses at 110.
    assert_eq!(epochs_finalized_at(109, 110, 10, 100), Some((0, 0)));
    // Parent already finalized epoch 0 ⇒ this block finalizes nothing new.
    assert_eq!(epochs_finalized_at(110, 111, 10, 100), None);
    // Next block to reach 120 finalizes epoch 1.
    assert_eq!(epochs_finalized_at(110, 120, 10, 100), Some((1, 1)));
    // A DAA jump finalizes a contiguous range at once (epochs 0,1,2 at 110/120/130).
    assert_eq!(epochs_finalized_at(109, 130, 10, 100), Some((0, 2)));
    // Early chain: parent below the first threshold ⇒ range starts at epoch 0.
    assert_eq!(epochs_finalized_at(50, 115, 10, 100), Some((0, 0)));
    // epoch_length 0 is clamped to 1 (no panic / div-by-zero).
    assert_eq!(epochs_finalized_at(0, 105, 0, 100), Some((0, 4)));
}

#[test]
fn active_bond_view_apply_insert_then_resolve() {
    let op = TransactionOutpoint::new(Hash64::from_bytes([0x11; 64]), 0);
    let mut view = ActiveBondView::new();
    assert!(view.is_empty());
    view.apply(&[BondMutation::Insert(op, fixture_bond_record(op))]);
    assert_eq!(view.len(), 1);
    // Active well past activation; not active before it.
    assert!(view.active_bond_at(&op, 10_000).is_some());
    assert!(view.active_bond_at(&op, 0).is_none());
    // Unknown outpoint resolves to None.
    let other = TransactionOutpoint::new(Hash64::from_bytes([0x22; 64]), 0);
    assert!(view.active_bond_at(&other, 10_000).is_none());
}

#[test]
fn active_bond_view_from_records_seeds_verbatim() {
    // Seeding from the store snapshot must preserve each record's fields
    // (incl. an already-slashed one) and resolve them correctly.
    let op1 = TransactionOutpoint::new(Hash64::from_bytes([0x11; 64]), 0);
    let op2 = TransactionOutpoint::new(Hash64::from_bytes([0x22; 64]), 0);
    let mut slashed = fixture_bond_record(op2);
    slashed.slashed_at_daa_score = Some(1);
    slashed.status = BondStatus::Slashed;
    let view = ActiveBondView::from_records([(op1, fixture_bond_record(op1)), (op2, slashed)]);
    assert_eq!(view.len(), 2);
    assert!(view.active_bond_at(&op1, 10_000).is_some());
    assert!(view.active_bond_at(&op2, 10_000).is_none()); // seeded as slashed

    // ADR-0018 §H: `records()` snapshots every bond (any status) for the per-branch
    // reorg-gate StakeScore walk — both the active and the slashed one.
    let outpoints: Vec<_> = view.records().iter().map(|r| r.bond_outpoint).collect();
    assert_eq!(outpoints.len(), 2);
    assert!(outpoints.contains(&op1) && outpoints.contains(&op2));
}

#[test]
fn active_bond_view_revert_insert_removes() {
    let op = TransactionOutpoint::new(Hash64::from_bytes([0x11; 64]), 0);
    let muts = vec![BondMutation::Insert(op, fixture_bond_record(op))];
    let mut view = ActiveBondView::new();
    view.apply(&muts);
    view.revert(&muts);
    assert!(view.is_empty());
    assert!(view.get(&op).is_none());
}

#[test]
fn active_bond_view_slash_then_revert_round_trips() {
    let op = TransactionOutpoint::new(Hash64::from_bytes([0x11; 64]), 0);
    let mut view = ActiveBondView::new();
    view.apply(&[BondMutation::Insert(op, fixture_bond_record(op))]);
    assert!(view.active_bond_at(&op, 10_000).is_some());

    // Slash: record becomes Slashed → not active at any DAA score.
    let slash = vec![BondMutation::Slash(op, 8_000)];
    view.apply(&slash);
    assert_eq!(view.get(&op).unwrap().status, BondStatus::Slashed);
    assert_eq!(view.get(&op).unwrap().slashed_at_daa_score, Some(8_000));
    assert!(view.active_bond_at(&op, 10_000).is_none());

    // Revert slash (mirrors stage_dns_bond_mutations: clears slash,
    // status → Active); time-based activation makes it active again.
    view.revert(&slash);
    assert_eq!(view.get(&op).unwrap().slashed_at_daa_score, None);
    assert!(view.active_bond_at(&op, 10_000).is_some());
}

#[test]
fn active_bond_view_multi_block_apply_then_reverse_revert_restores_consensus_state() {
    // Apply blocks b1, b2 then revert b2 (reverse chain order) → the
    // post-b1 *consensus state* is restored, exactly like a UTXO reorg.
    //
    // Note: equality is asserted over the consensus-relevant queries
    // (existence + `active_bond_at`), NOT full struct equality. A
    // Slash→revert leaves the cosmetic `status` enum at `Active` even
    // if the bond was `Pending` pre-slash — this faithfully mirrors
    // `stage_dns_bond_mutations` (the persisted store does the same),
    // and is consensus-invisible because every read goes through
    // `effective_bond_status`, which derives status purely from the
    // DAA-stamped fields and ignores the stored `status` enum.
    let op1 = TransactionOutpoint::new(Hash64::from_bytes([0x11; 64]), 0);
    let op2 = TransactionOutpoint::new(Hash64::from_bytes([0x22; 64]), 0);
    let b1 = vec![BondMutation::Insert(op1, fixture_bond_record(op1))];
    let b2 = vec![BondMutation::Insert(op2, fixture_bond_record(op2)), BondMutation::Slash(op1, 9_000)];

    let mut view = ActiveBondView::new();
    view.apply(&b1);
    view.apply(&b2);
    assert_eq!(view.len(), 2);
    assert!(view.active_bond_at(&op1, 10_000).is_none()); // slashed in b2
    assert!(view.active_bond_at(&op2, 10_000).is_some());

    // Revert b2 (most-recent first) → post-b1 consensus state.
    view.revert(&b2);
    assert_eq!(view.len(), 1);
    assert!(view.get(&op2).is_none()); // op2's Insert reverted
    assert!(view.active_bond_at(&op1, 10_000).is_some()); // slash cleared
    assert_eq!(view.get(&op1).unwrap().owner_reward_spk_payload, fixture_bond_record(op1).owner_reward_spk_payload);
}

#[test]
fn rewarded_epoch_set_insert_contains_remove() {
    let op = TransactionOutpoint::new(Hash64::from_bytes([0x11; 64]), 0);
    let mut set = RewardedEpochSet::new();
    assert!(!set.contains(&op, 7));
    assert!(set.insert(op, 7)); // newly inserted
    assert!(set.contains(&op, 7));
    assert!(!set.insert(op, 7)); // duplicate → false, not rewarded again
    assert_eq!(set.len(), 1);
    assert!(set.remove(&op, 7)); // reorg reverse
    assert!(!set.contains(&op, 7));
    assert!(set.is_empty());
}

#[test]
fn rewarded_epoch_set_keys_on_both_outpoint_and_epoch() {
    let op1 = TransactionOutpoint::new(Hash64::from_bytes([0x11; 64]), 0);
    let op2 = TransactionOutpoint::new(Hash64::from_bytes([0x22; 64]), 0);
    let mut set = RewardedEpochSet::new();
    set.insert(op1, 1);
    // Same outpoint, different epoch → distinct (a later epoch is payable).
    assert!(!set.contains(&op1, 2));
    // Different outpoint, same epoch → distinct.
    assert!(!set.contains(&op2, 1));
    set.insert(op1, 2);
    set.insert(op2, 1);
    assert_eq!(set.len(), 3);
}

// ---- A.5: StakeScore aggregation -> DnsState ----

#[test]
fn aggregate_epoch_tallies_dedups_triple_and_normalises() {
    let op1 = TransactionOutpoint::new(Hash64::from_bytes([0x01; 64]), 0);
    let op2 = TransactionOutpoint::new(Hash64::from_bytes([0x02; 64]), 0);
    let v1 = Hash64::from_bytes([0xa1; 64]);
    let v2 = Hash64::from_bytes([0xa2; 64]);
    let contribs = vec![
        AttestationContribution { epoch: 1, validator_id: v1, bond_outpoint: op1, signed_weight: 30 },
        // Duplicate (op1, v1, epoch 1) — must NOT be double-counted.
        AttestationContribution { epoch: 1, validator_id: v1, bond_outpoint: op1, signed_weight: 30 },
        AttestationContribution { epoch: 1, validator_id: v2, bond_outpoint: op2, signed_weight: 20 },
        AttestationContribution { epoch: 2, validator_id: v1, bond_outpoint: op1, signed_weight: 30 },
    ];
    let totals = BTreeMap::from([(1u64, 100u128), (2u64, 100u128), (3u64, 100u128)]);
    let tallies = aggregate_epoch_tallies(&contribs, &totals);
    assert_eq!(tallies.len(), 3); // ascending by epoch
    assert_eq!(tallies[0], EpochStakeTally { epoch: 1, signed_weight: 50, total_weight: 100 });
    assert_eq!(tallies[1], EpochStakeTally { epoch: 2, signed_weight: 30, total_weight: 100 });
    assert_eq!(tallies[2], EpochStakeTally { epoch: 3, signed_weight: 0, total_weight: 100 });
    // End-to-end (φS = 0, linear): 0.5 + 0.3 + 0.0 = 0.8.
    assert_eq!(
        compute_stake_score(&tallies, EpochCreditRule::QualityFloor { quality_floor_bps: 0 }),
        StakeScore(STAKE_SCORE_SCALE / 2 + STAKE_SCORE_SCALE * 3 / 10)
    );
}

#[test]
fn aggregate_epoch_tallies_ignores_epoch_without_denominator() {
    let op = TransactionOutpoint::new(Hash64::from_bytes([0x01; 64]), 0);
    let contribs =
        vec![AttestationContribution { epoch: 9, validator_id: Hash64::from_bytes([0xa1; 64]), bond_outpoint: op, signed_weight: 50 }];
    assert!(aggregate_epoch_tallies(&contribs, &BTreeMap::new()).is_empty());
}

#[test]
fn advance_dns_confirmation_confirms_canonical_anchor_not_sink() {
    let vsc = Hash64::from_bytes([0x22; 64]);
    let (cw, cs) = (BlueWorkType::from_u64(1000), StakeScore(STAKE_SCORE_SCALE)); // work>=1000, stake>=1.0
    let stage = DnsRolloutStage::Active;

    // Stake below cS, no prev -> nothing confirmed (zero), even with a candidate anchor.
    let sink1 = Hash64::from_bytes([0x11; 64]);
    let canon1 = Hash64::from_bytes([0x91; 64]);
    let s1 = advance_dns_confirmation(
        None,
        sink1,
        500,
        Some((canon1, 480)),
        BlueWorkType::from_u64(2000),
        StakeScore(STAKE_SCORE_SCALE / 2),
        stage,
        vsc,
        DnsHealth::Active,
        cw,
        cs,
        // anchor's own epoch has live support (this test covers anchor SELECTION, not the guard)
        1,
        1,
    );
    assert_eq!(s1.selected_chain_anchor, sink1);
    assert_eq!(s1.last_dns_confirmed_anchor, Hash64::default());

    // audit #3: thresholds met -> confirm the CANONICAL anchor, NOT the POV-dependent sink.
    let sink2 = Hash64::from_bytes([0x33; 64]);
    let canon2 = Hash64::from_bytes([0x99; 64]);
    let s2 = advance_dns_confirmation(
        Some(&s1),
        sink2,
        600,
        Some((canon2, 580)),
        BlueWorkType::from_u64(2000),
        StakeScore(STAKE_SCORE_SCALE),
        stage,
        vsc,
        DnsHealth::DegradedStakeQualityLow,
        cw,
        cs,
        // anchor's own epoch has live support (this test covers anchor SELECTION, not the guard)
        1,
        1,
    );
    assert_eq!(s2.selected_chain_anchor, sink2, "selected_chain_anchor stays the sink (throttle only)");
    assert_eq!(s2.last_dns_confirmed_anchor, canon2, "confirmed anchor is the canonical anchor");
    assert_eq!(s2.last_dns_confirmed_anchor_daa_score, 580);
    assert_ne!(s2.last_dns_confirmed_anchor, sink2, "the sink is NOT what gets confirmed");
    assert_eq!(s2.health, DnsHealth::DegradedStakeQualityLow); // health never gates confirmation

    // No canonical anchor (None) -> cannot confirm even if depth passes; carry prev forward.
    let sink3 = Hash64::from_bytes([0x44; 64]);
    let s3 = advance_dns_confirmation(
        Some(&s2),
        sink3,
        700,
        None,
        BlueWorkType::from_u64(2000),
        StakeScore(STAKE_SCORE_SCALE),
        stage,
        vsc,
        DnsHealth::Active,
        cw,
        cs,
        // anchor's own epoch has live support (this test covers anchor SELECTION, not the guard)
        1,
        1,
    );
    assert_eq!(s3.selected_chain_anchor, sink3);
    assert_eq!(s3.last_dns_confirmed_anchor, canon2, "no ready anchor -> keep prev confirmed");

    // Below-threshold stake (with a candidate anchor present) also carries prev forward.
    let s4 = advance_dns_confirmation(
        Some(&s2),
        sink3,
        700,
        Some((Hash64::from_bytes([0x77; 64]), 690)),
        BlueWorkType::from_u64(2000),
        StakeScore(0),
        stage,
        vsc,
        DnsHealth::Active,
        cw,
        cs,
        // anchor's own epoch has live support (this test covers anchor SELECTION, not the guard)
        1,
        1,
    );
    assert_eq!(s4.last_dns_confirmed_anchor, canon2, "below-threshold -> keep prev confirmed");
    assert_eq!(s4.last_dns_confirmed_anchor_daa_score, 580);
}

/// "Dead-branch confirm" guard (incident 2026-08-03 §8). `stake_depth` is a WINDOWED sum over
/// several epochs, so a branch whose validators have already gone silent still carries enough
/// accrued stake to clear `required_stake_depth`. Without this guard it would keep latching
/// NEW anchors from that stale stake, arming an unreleasable reorg veto on a branch nothing is
/// attesting to. An anchor must have live support in its OWN epoch to become a veto.
#[test]
fn anchor_without_live_support_is_not_confirmed() {
    let vsc = Hash64::from_bytes([0x22; 64]);
    let (cw, cs) = (BlueWorkType::from_u64(1000), StakeScore(STAKE_SCORE_SCALE));
    let canon = Hash64::from_bytes([0xC0; 64]);
    let confirm = |attesters: u32, min_attesters: u32| {
        advance_dns_confirmation(
            None,
            Hash64::from_bytes([0x10; 64]),
            900,
            Some((canon, 880)),
            BlueWorkType::from_u64(2000),  // work clears cW
            StakeScore(STAKE_SCORE_SCALE), // windowed stake clears cS (accrued in EARLIER epochs)
            DnsRolloutStage::Active,
            vsc,
            DnsHealth::Active,
            cw,
            cs,
            attesters,
            min_attesters,
        )
        .last_dns_confirmed_anchor
    };
    // Both depth dimensions clear, but the anchor's own epoch has no credited attestation.
    assert_eq!(confirm(0, 1), Hash64::default(), "no live support in the anchor's epoch ⇒ NOT confirmed");
    // Same inputs with one credited attestation in the anchor's epoch ⇒ confirmed as before.
    assert_eq!(confirm(1, 1), canon, "live support ⇒ confirmed");
    // Distinct-attester floor: a lone signer must not arm a network-wide veto where the active
    // set is meant to be multi-operator (mainnet's `min_anchor_attesters = 2`).
    assert_eq!(confirm(1, 2), Hash64::default(), "one attester below a 2-attester floor ⇒ NOT confirmed");
    assert_eq!(confirm(2, 2), canon, "two distinct attesters meet the floor ⇒ confirmed");
    assert_eq!(confirm(5, 2), canon, "more than the floor is fine");
    // A misconfigured `0` floor must not degrade into "confirm with no support at all".
    assert_eq!(confirm(0, 0), Hash64::default(), "a zero floor still requires ≥1 credited attestation");
}

/// audit H-02 (true WorkDepth, Option A): with `required_work_depth > 0`, confirmation is
/// genuinely TWO-DIMENSIONAL — it requires BOTH `WorkDepth ≥ cW` AND `StakeDepth ≥ cS`. The
/// decisive new property: a candidate with ENOUGH STAKE but SHALLOW work (`WorkDepth < cW`) is
/// NOT confirmed (a stake-side adversary can no longer fast-finalize a low-PoW anchor). This is
/// the behavior that `required_work_depth = 0` (devnet/simnet) collapses to "stake-only".
#[test]
fn true_workdepth_requires_both_work_and_stake() {
    let vsc = Hash64::from_bytes([0x22; 64]);
    let (cw, cs) = (BlueWorkType::from_u64(1000), StakeScore(STAKE_SCORE_SCALE)); // cW=1000, cS=1.0
    let stage = DnsRolloutStage::Active;
    let canon = Hash64::from_bytes([0xC0; 64]);
    let confirm = |work: u64, stake: u128| {
        advance_dns_confirmation(
            None,
            Hash64::from_bytes([0x10; 64]),
            900,
            Some((canon, 880)),
            BlueWorkType::from_u64(work),
            StakeScore(stake),
            stage,
            vsc,
            DnsHealth::Active,
            cw,
            cs,
            1,
            1,
        )
        .last_dns_confirmed_anchor
    };
    // Both dimensions clear ⇒ confirmed.
    assert_eq!(confirm(2000, STAKE_SCORE_SCALE), canon, "work≥cW ∧ stake≥cS ⇒ confirmed");
    // Enough STAKE but SHALLOW work ⇒ NOT confirmed (the true-WorkDepth gate; stake-only would confirm).
    assert_eq!(confirm(999, STAKE_SCORE_SCALE), Hash64::default(), "stake≥cS but work<cW ⇒ NOT confirmed (true WorkDepth)");
    // Enough WORK but insufficient stake ⇒ NOT confirmed (the stake gate).
    assert_eq!(confirm(2000, STAKE_SCORE_SCALE / 2), Hash64::default(), "work≥cW but stake<cS ⇒ NOT confirmed");
    // Exactly at both thresholds ⇒ confirmed (inclusive ≥).
    assert_eq!(confirm(1000, STAKE_SCORE_SCALE), canon, "work==cW ∧ stake==cS ⇒ confirmed");
}

#[test]
fn total_active_stake_by_epoch_sums_only_active_bonds() {
    // A: activation 100, stake 30. B: activation 500, stake 20.
    // C: activation 100, slashed at 300, stake 50.
    let mut a = stake_bond_record_from_payload(&fixture_bond(), fixture_outpoint());
    a.amount = 30;
    a.activation_daa_score = 100;
    let mut b = a.clone();
    b.amount = 20;
    b.activation_daa_score = 500;
    let mut c = a.clone();
    c.amount = 50;
    c.slashed_at_daa_score = Some(300);
    let bonds = vec![a, b, c];

    let epochs = BTreeMap::from([(1u64, 50u64), (2, 200), (3, 400), (4, 600)]);
    let totals = total_active_stake_by_epoch(&bonds, &epochs);
    assert_eq!(totals.get(&1), Some(&0)); // daa 50: all activate >= 100 -> Pending
    assert_eq!(totals.get(&2), Some(&80)); // daa 200: A(30) + C(50) active
    assert_eq!(totals.get(&3), Some(&30)); // daa 400: A(30); C slashed @300; B not yet
    assert_eq!(totals.get(&4), Some(&50)); // daa 600: A(30) + B(20); C slashed
}

#[test]
fn attestations_from_accepted_txs_flattens_shards_only() {
    // Two shards (2 + 3 attestations) + a non-overlay tx -> 5 attestations.
    let shard_a = dns_overlay_tx(SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, borsh::to_vec(&fixture_shard(2)).unwrap());
    let shard_b = dns_overlay_tx(SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, borsh::to_vec(&fixture_shard(3)).unwrap());
    let native = dns_overlay_tx(SubnetworkId::from_byte(0), vec![1, 2, 3]);
    let bond = dns_overlay_tx(SUBNETWORK_ID_STAKE_BOND, borsh::to_vec(&fixture_bond()).unwrap());
    let atts = attestations_from_accepted_txs(&[shard_a, native, shard_b, bond]);
    assert_eq!(atts.len(), 5);
    assert!(atts.iter().all(|a| a.signature.len() == STAKE_ATTESTATION_SIG_LEN));

    // Undecodable shard payload is skipped.
    let bad = dns_overlay_tx(SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, vec![0xff]);
    assert!(attestations_from_accepted_txs(&[bad]).is_empty());
}

#[test]
fn dns_confirmation_from_state_sets_flags() {
    let state = DnsState {
        selected_chain_anchor: Hash64::from_bytes([0x11; 64]),
        anchor_daa_score: 1000,
        work_depth: BlueWorkType::from_u64(5000),
        stake_depth: StakeScore(STAKE_SCORE_SCALE), // 1.0
        last_dns_confirmed_anchor: Hash64::default(),
        last_dns_confirmed_anchor_daa_score: 0,
        rollout_stage: DnsRolloutStage::Active,
        validator_set_commitment: Hash64::from_bytes([0x22; 64]),
        health: DnsHealth::DegradedCertificateCensored,
    };
    // Both thresholds met -> pow + dns confirmed.
    let c = dns_confirmation_from_state(&state, BlueWorkType::from_u64(1000), StakeScore(STAKE_SCORE_SCALE / 2));
    assert_eq!(c.block_hash, state.selected_chain_anchor);
    assert!(c.pow_confirmed && c.dns_confirmed);
    assert_eq!(c.rollout_stage, DnsRolloutStage::Active);
    assert_eq!(c.health, DnsHealth::DegradedCertificateCensored); // surfaced verbatim from DnsState
    assert_eq!(c.required_work_depth, BlueWorkType::from_u64(1000));

    // Work met but stake below threshold -> pow only.
    let c = dns_confirmation_from_state(&state, BlueWorkType::from_u64(1000), StakeScore(STAKE_SCORE_SCALE * 2));
    assert!(c.pow_confirmed && !c.dns_confirmed);

    // Work below threshold -> neither.
    let c = dns_confirmation_from_state(&state, BlueWorkType::from_u64(9999), StakeScore(0));
    assert!(!c.pow_confirmed && !c.dns_confirmed);
}

#[test]
fn dns_params_borsh_roundtrip() {
    let params = DnsParams {
        dns_activation_daa_score: 1_000_000,
        inactivity_leak_daa: 5_040,
        min_active_stake_sompi: 10_000_000_000_000,
        min_active_validators: 32,
        min_bond_amount_sompi: 2_000_000_000_000,
        epoch_length_blocks: 600,
        required_work_depth: BlueWorkType::from_u64(1_000_000),
        required_stake_depth: StakeScore(10 * STAKE_SCORE_SCALE),
        emergency_work_margin: BlueWorkType::from_u64(10_000_000),
        emergency_stake_margin: StakeScore(100 * STAKE_SCORE_SCALE),
        emergency_work_override_multiplier: 4,
        stake_preference_max_work_deficit_multiplier: 2,
        coinbase_settlement_long_maturity_daa: 30_000,
        coinbase_settlement_consensus_activation_daa_score: u64::MAX,
        unbond_authz_mergeset_activation_daa_score: 0,
        max_reorg_horizon_blocks: 100_000,
        evidence_window_blocks: 200_000,
        unbonding_period_blocks: 350_000, // > R + E
        max_attestations_per_block: MAX_ATTESTATIONS_PER_SHARD as u16,
        max_attestation_shard_mass: 50_000,
        reward_uniqueness_window_blocks: 3_600, // ~6 epochs (epoch_length 600)
        stake_event_quality_floor_bps: 6000,    // ADR-0018 §B (φS = 0.60)
        degraded_stake_quality_epochs: 4,       // ADR-0018 §C
        stake_censorship_floor_bps: 1000,       // ADR-0018 §C (0.10)
        reward_params: RewardParams {
            per_attestation_reward_sompi: 100_000_000,
            slashing_reporter_reward_bps: 1000,
            max_validator_inflation_per_block_sompi: 100_000_000 * MAX_ATTESTATIONS_PER_SHARD as u64,
            validator_participation_bps: 10000,
            validator_quality_bonus_bps: 0,
            quality_gate_bonus_sompi: 50_000_000,
            worker_urgency_multiplier_scaled: STAKE_SCORE_SCALE as u64,
            fee_split: fixture_fee_split(),
            fee_split_bootstrap: fixture_fee_split_bootstrap(),
            security_reserve_bps: 2000,
            victim_epoch_pool_bps: 1000,
            reserve_drip_per_epoch_cap_sompi: 1_000_000,
        },
        reorg_mode: DnsReorgMode::TwoDimensionalDominance,
        full_reward_split_daa_score: 2_000_000,
        pos_v2_activation_daa_score: 3_000_000,
        attestation_epoch_length_blue_score: 4_000_000,
        attestation_lag_blue_score: 5_000_000,
        attestation_anchor_backoff_blue_score: 6_000_000,
        stake_score_window_blue_score: 7_000_000,
        finality_fee_activation_daa_score: 8_000_000,
        bond_spend_gate_mergeset_activation_daa_score: 9_000_000,
        mandatory_attestation_inclusion_daa_score: 10_000_000,
        bridge_finality_max_staleness_daa_score: 11_000_000,
        // Non-default VLT values so the borsh round-trip actually exercises the appended
        // fields (an all-default `INERT` would round-trip even if the encoding dropped them).
        vlt: VltParams {
            vlt_shadow_activation_daa_score: 11_500_000,
            vlt_activation_daa_score: 12_000_000,
            credit_window_epochs: 42,
            ..VltParams::INERT
        },
        tkn: crate::token::TokenParams {
            tkn_shadow_activation_daa_score: 12_500_000,
            tkn_activation_daa_score: 13_000_000,
            emission_epoch_budget_r0_atomic: 500 * 100_000_000,
            ..crate::token::TokenParams::INERT
        },
        vlt_credit_window_blue_score: 13_000_000,
        dns_gate_horizon_blocks: 14_000_000,
        dns_veto_ttl_daa_score: 15_000_000,
        min_anchor_attesters: 3,
    };
    let bytes = borsh::to_vec(&params).unwrap();
    let back: DnsParams = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, params);

    // ADR-0009 §"Long-range bound" requires U >= R + E.
    assert!(params.unbonding_period_blocks >= params.max_reorg_horizon_blocks + params.evidence_window_blocks);
    // ADR-0018 §E: the validator pool split is a partition (the two shares sum to 100%).
    assert_eq!(params.reward_params.validator_participation_bps + params.reward_params.validator_quality_bonus_bps, 10_000);
}

#[test]
fn dns_confirmation_borsh_roundtrip() {
    let c = DnsConfirmation {
        block_hash: Hash64::from_bytes([0x99u8; 64]),
        work_depth: BlueWorkType::from_u64(42),
        required_work_depth: BlueWorkType::from_u64(10),
        stake_depth: StakeScore(500_000_000),
        required_stake_depth: StakeScore(STAKE_SCORE_SCALE),
        pow_confirmed: true,
        dns_confirmed: false,
        rollout_stage: DnsRolloutStage::Bootstrap,
        expected_dns_confirmation_seconds: 600,
        work_reorg_risk_upper_bound: "n/a".into(),
        stake_reorg_risk_upper_bound: "n/a".into(),
        dns_reorg_risk_conservative_bound: "n/a".into(),
        note: "Phase 10 stub".into(),
        health: DnsHealth::DegradedStakeQualityLow,
        last_dns_confirmed_anchor: Hash64::from_bytes([0x77u8; 64]),
        last_dns_confirmed_anchor_daa_score: 12345,
    };
    let bytes = borsh::to_vec(&c).unwrap();
    let back: DnsConfirmation = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, c);
}

#[test]
fn dns_rollout_stage_default_is_launch() {
    // The default rollout stage must be `Launch` so a node booting
    // before any DNS parameters are configured behaves as a pure
    // PoW kaspa-pq node. ADR-0009 §"Three-stage rollout".
    assert_eq!(DnsRolloutStage::default(), DnsRolloutStage::Launch);
}

#[test]
fn bond_status_default_is_pending() {
    // A freshly-recorded bond is `Pending` until
    // `activation_daa_score` is crossed. Matches ADR-0010
    // §"Validator service runtime" predicate ordering.
    assert_eq!(BondStatus::default(), BondStatus::Pending);
}

#[test]
fn stake_bond_record_borsh_roundtrip() {
    let rec = StakeBondRecord {
        version: DNS_PAYLOAD_VERSION_V1,
        bond_outpoint: fixture_outpoint(),
        owner_pubkey_hash: Hash64::from_bytes([0xaau8; 64]),
        validator_pubkey_hash: Hash64::from_bytes([0xbbu8; 64]),
        validator_pubkey: vec![0xccu8; STAKE_VALIDATOR_PUBKEY_LEN],
        amount: 100_000_000_000,
        activation_daa_score: 5_000,
        created_daa_score: 4_900,
        unbonding_period_blocks: 100_000,
        owner_reward_spk_payload: [0xddu8; 64],
        unbond_request_daa_score: Some(123_456),
        slashed_at_daa_score: None,
        status: BondStatus::Unbonding,
    };
    let bytes = borsh::to_vec(&rec).unwrap();
    let back: StakeBondRecord = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, rec);
}

#[test]
fn validator_record_borsh_roundtrip() {
    let v = ValidatorRecord { validator_id: Hash64::from_bytes([0x42u8; 64]), stake_amount: 1_000_000, activation_daa_score: 99 };
    let bytes = borsh::to_vec(&v).unwrap();
    let back: ValidatorRecord = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, v);
}

#[test]
fn validator_set_snapshot_borsh_roundtrip() {
    let snap = ValidatorSetSnapshot { epoch: 7, validators: fixture_validators() };
    let bytes = borsh::to_vec(&snap).unwrap();
    let back: ValidatorSetSnapshot = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, snap);
}

#[test]
fn dns_state_borsh_roundtrip() {
    let s = DnsState {
        selected_chain_anchor: Hash64::from_bytes([0x55u8; 64]),
        anchor_daa_score: 1_000,
        work_depth: BlueWorkType::from_u64(500),
        stake_depth: StakeScore(2 * STAKE_SCORE_SCALE),
        last_dns_confirmed_anchor: Hash64::from_bytes([0x66u8; 64]),
        last_dns_confirmed_anchor_daa_score: 900,
        rollout_stage: DnsRolloutStage::Active,
        validator_set_commitment: Hash64::from_bytes([0x77u8; 64]),
        health: DnsHealth::DegradedCertificateCensored,
    };
    let bytes = borsh::to_vec(&s).unwrap();
    let back: DnsState = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, s);
}

#[test]
fn block_template_policy_borsh_roundtrip() {
    let p = BlockTemplatePolicy {
        max_attestations_per_block: MAX_ATTESTATIONS_PER_SHARD as u16,
        max_attestation_shard_mass: 50_000,
        reserve_mass_for_normal_txs: 200_000,
    };
    let bytes = borsh::to_vec(&p).unwrap();
    let back: BlockTemplatePolicy = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, p);
}

// ---- validator_set_commitment ---------------------------------

#[test]
fn validator_set_commitment_is_order_independent() {
    // Same set in three different input orders. ADR-0010
    // §"Validator-set commitment derivation" guarantees the
    // result is order-independent because the helper sorts a
    // clone before hashing.
    let a = fixture_validators();
    let mut b = a.clone();
    b.reverse();
    let mut c = a.clone();
    c.swap(0, 2);

    let ca = validator_set_commitment(11, &a);
    let cb = validator_set_commitment(11, &b);
    let cc = validator_set_commitment(11, &c);
    assert_eq!(ca, cb);
    assert_eq!(ca, cc);
}

#[test]
fn validator_set_commitment_changes_with_epoch() {
    // Same validator set, different epochs => different
    // commitments. Guards against an attestation replay across
    // epoch boundaries.
    let v = fixture_validators();
    assert_ne!(validator_set_commitment(0, &v), validator_set_commitment(1, &v));
    assert_ne!(validator_set_commitment(11, &v), validator_set_commitment(12, &v));
}

#[test]
fn validator_set_commitment_changes_with_membership() {
    // Removing a validator must change the commitment.
    let full = fixture_validators();
    let mut subset = full.clone();
    subset.pop();
    assert_ne!(validator_set_commitment(11, &full), validator_set_commitment(11, &subset));
}

#[test]
fn validator_set_commitment_changes_with_stake_amount() {
    // Bumping any field a validator contributes (stake_amount,
    // activation_daa_score) must change the commitment.
    let baseline = fixture_validators();
    let mut bumped_stake = baseline.clone();
    bumped_stake[0].stake_amount += 1;
    assert_ne!(validator_set_commitment(11, &baseline), validator_set_commitment(11, &bumped_stake));

    let mut bumped_daa = baseline.clone();
    bumped_daa[0].activation_daa_score += 1;
    assert_ne!(validator_set_commitment(11, &baseline), validator_set_commitment(11, &bumped_daa));
}

#[test]
fn validator_set_commitment_empty_is_well_defined() {
    // Empty validator set still produces a deterministic hash
    // (not all-zero): epoch || u32::LE(0).
    let c0 = validator_set_commitment(0, &[]);
    let c1 = validator_set_commitment(1, &[]);
    assert_ne!(c0, c1);
    // Must not collide with the all-zero `Hash64` sentinel.
    assert_ne!(c0, kaspa_hashes::ZERO_HASH64);
}

#[test]
fn validator_set_commitment_matches_adr_byte_layout() {
    // Pin one full byte-layout to a known-good value. Any future
    // change to the ADR-0010 derivation (field order, length
    // prefix encoding, domain-separator key) trips this test
    // immediately — the value is consensus-stable and any drift
    // is a hard fork.
    let v = vec![
        ValidatorRecord { validator_id: Hash64::from_bytes([0x01u8; 64]), stake_amount: 1, activation_daa_score: 2 },
        ValidatorRecord { validator_id: Hash64::from_bytes([0x02u8; 64]), stake_amount: 3, activation_daa_score: 4 },
    ];
    // Re-derive the expected value with a hand-rolled hasher,
    // matching the ADR text byte-for-byte. Equality here is the
    // "two independent implementations agree" sanity check.
    let mut h = Blake2bParams::new().hash_length(64).key(VALIDATOR_SET_COMMITMENT_KEY).to_state();
    h.update(&5u64.to_le_bytes()); // epoch = 5
    h.update(&2u32.to_le_bytes()); // len   = 2
    // Sorted by validator_id ascending: [0x01..], [0x02..].
    h.update(&[0x01u8; 64]);
    h.update(&1u64.to_le_bytes());
    h.update(&2u64.to_le_bytes());
    h.update(&[0x02u8; 64]);
    h.update(&3u64.to_le_bytes());
    h.update(&4u64.to_le_bytes());
    let mut expected = [0u8; 64];
    expected.copy_from_slice(h.finalize().as_bytes());

    let actual = validator_set_commitment(5, &v);
    assert_eq!(actual.as_bytes(), expected);
}

#[test]
fn validator_id_from_pubkey_is_unkeyed_blake2b_512() {
    // Canonical derivation = unkeyed BLAKE2b-512 of the public key
    // (ADR-0008/0012). Pinning it guards against accidental keying or a
    // switch to the 32-byte P2PKH address payload — either would be a hard fork.
    let pubkey = [0x42u8; 2592]; // MLDSA87_PK_LEN-sized sample
    let mut expected = [0u8; 64];
    expected.copy_from_slice(Blake2bParams::new().hash_length(64).to_state().update(&pubkey).finalize().as_bytes());
    let id = validator_id_from_pubkey(&pubkey);
    assert_eq!(id, Hash64::from_bytes(expected));
    // Deterministic and input-sensitive.
    assert_eq!(validator_id_from_pubkey(&pubkey), id);
    let mut other = pubkey;
    other[0] ^= 0x01;
    assert_ne!(validator_id_from_pubkey(&other), id);
}

#[test]
fn signature_fingerprint_is_unkeyed_blake2b_512() {
    let sig = [0x7eu8; 4627]; // MLDSA87_SIG_LEN-sized sample
    let mut expected = [0u8; 64];
    expected.copy_from_slice(Blake2bParams::new().hash_length(64).to_state().update(&sig).finalize().as_bytes());
    assert_eq!(signature_fingerprint(&sig), Hash64::from_bytes(expected));
    // Input-sensitive (so a re-broadcast of a *different* signature is distinguishable).
    let mut other = sig;
    other[0] ^= 0x01;
    assert_ne!(signature_fingerprint(&other), signature_fingerprint(&sig));
}

// ---- stake_attestation_message --------------------------------

#[test]
fn stake_attestation_message_is_deterministic() {
    let target = Hash64::from_bytes([0x11u8; 64]);
    let vsc = Hash64::from_bytes([0x22u8; 64]);
    let op = fixture_outpoint();
    let a = stake_attestation_message(b"kaspa-pq-devnet", 7, target, 1_234_567, vsc, op);
    let b = stake_attestation_message(b"kaspa-pq-devnet", 7, target, 1_234_567, vsc, op);
    assert_eq!(a, b);
}

#[test]
fn stake_attestation_message_changes_with_each_field() {
    // ADR-0009 Addendum A.3: every input — including network_id and
    // bond_outpoint — must perturb the digest.
    let net = b"kaspa-pq-devnet".as_slice();
    let th = Hash64::from_bytes([0x11u8; 64]);
    let vsc = Hash64::from_bytes([0x22u8; 64]);
    let op = TransactionOutpoint::new(Hash64::from_bytes([0x77u8; 64]), 42);
    let base = stake_attestation_message(net, 7, th, 100, vsc, op);
    // network_id (A.3 — guards cross-network replay).
    assert_ne!(base, stake_attestation_message(b"kaspa-pq-testnet", 7, th, 100, vsc, op));
    // Epoch.
    assert_ne!(base, stake_attestation_message(net, 8, th, 100, vsc, op));
    // target_hash.
    assert_ne!(base, stake_attestation_message(net, 7, Hash64::from_bytes([0x12u8; 64]), 100, vsc, op));
    // target_daa_score.
    assert_ne!(base, stake_attestation_message(net, 7, th, 101, vsc, op));
    // validator_set_commitment.
    assert_ne!(base, stake_attestation_message(net, 7, th, 100, Hash64::from_bytes([0x23u8; 64]), op));
    // bond_outpoint transaction_id (A.3 — guards cross-bond replay).
    assert_ne!(base, stake_attestation_message(net, 7, th, 100, vsc, TransactionOutpoint::new(Hash64::from_bytes([0x78u8; 64]), 42)));
    // bond_outpoint index.
    assert_ne!(base, stake_attestation_message(net, 7, th, 100, vsc, TransactionOutpoint::new(Hash64::from_bytes([0x77u8; 64]), 43)));
}

#[test]
fn stake_attestation_message_uses_attestation_domain_key_and_full_layout() {
    // Reconstruct the exact Addendum A.3 layout and verify (a) the
    // attestation domain key differs from the tx domain key, and (b)
    // `stake_attestation_message` matches the byte-for-byte layout.
    let net = b"kaspa-pq-devnet".as_slice();
    let op = TransactionOutpoint::new(Hash64::from_bytes([0x77u8; 64]), 42);
    let inputs = |key: &[u8]| {
        let mut h = Blake2bParams::new().hash_length(32).key(key).to_state();
        h.update(net);
        h.update(&7u64.to_le_bytes());
        h.update(&[0x11u8; 64]);
        h.update(&100u64.to_le_bytes());
        h.update(&[0x22u8; 64]);
        h.update(&[0x77u8; 64]); // bond_outpoint.transaction_id
        h.update(&42u32.to_le_bytes()); // bond_outpoint.index
        h.finalize()
    };
    let with_att_key = inputs(ATTESTATION_MESSAGE_DOMAIN);
    let with_tx_key = inputs(b"kaspa-pq-v1/tx/mldsa87");
    assert_ne!(with_att_key.as_bytes(), with_tx_key.as_bytes());

    let actual = stake_attestation_message(net, 7, Hash64::from_bytes([0x11u8; 64]), 100, Hash64::from_bytes([0x22u8; 64]), op);
    assert_eq!(actual.as_bytes(), with_att_key.as_bytes());
}

// ---- Validator-local state (ADR-0011) -------------------------

fn fixture_signed() -> SignedEpochRecord {
    SignedEpochRecord {
        epoch: 7,
        target_hash: Hash64::from_bytes([0x11u8; 64]),
        target_daa_score: 1_234_567,
        signature_fingerprint: Hash64::from_bytes([0xabu8; 64]),
    }
}

#[test]
fn validator_status_default_is_node_not_synced() {
    // A freshly-started validator must default to
    // `NodeNotSynced` so it cannot take any sign-eligible
    // action before its runtime loop has confirmed the local
    // node is at tip. ADR-0011 §"Validator status enum".
    assert_eq!(ValidatorStatus::default(), ValidatorStatus::NodeNotSynced);
}

#[test]
fn validator_status_discriminants_are_api_stable() {
    // The Borsh discriminant of each variant is API-stable —
    // RPC clients persist these to disk. Any reorder is a
    // wire-format break. Pin the integer values so the test
    // trips immediately on accidental drift. Variants 0..8
    // are pinned per ADR-0011; variant 9 is appended per
    // ADR-0014 §"`ValidatorStatus` extension".
    assert_eq!(ValidatorStatus::NodeNotSynced as u8, 0);
    assert_eq!(ValidatorStatus::BondNotFound as u8, 1);
    assert_eq!(ValidatorStatus::BondPending as u8, 2);
    assert_eq!(ValidatorStatus::ActiveIdle as u8, 3);
    assert_eq!(ValidatorStatus::ActiveEligible as u8, 4);
    assert_eq!(ValidatorStatus::SignedThisEpoch as u8, 5);
    assert_eq!(ValidatorStatus::Unbonding as u8, 6);
    assert_eq!(ValidatorStatus::Slashed as u8, 7);
    assert_eq!(ValidatorStatus::DryRun as u8, 8);
    assert_eq!(ValidatorStatus::AwaitingTakeoverToken as u8, 9);
}

#[test]
fn validator_status_all_variants_borsh_roundtrip() {
    for v in [
        ValidatorStatus::NodeNotSynced,
        ValidatorStatus::BondNotFound,
        ValidatorStatus::BondPending,
        ValidatorStatus::ActiveIdle,
        ValidatorStatus::ActiveEligible,
        ValidatorStatus::SignedThisEpoch,
        ValidatorStatus::Unbonding,
        ValidatorStatus::Slashed,
        ValidatorStatus::DryRun,
        ValidatorStatus::AwaitingTakeoverToken,
    ] {
        let bytes = borsh::to_vec(&v).unwrap();
        let back: ValidatorStatus = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, v, "variant {v:?} did not round-trip through Borsh");
        assert_eq!(bytes.len(), 1, "ValidatorStatus must encode as a single byte");
    }
}

#[test]
fn signed_epoch_record_borsh_roundtrip() {
    let r = fixture_signed();
    let bytes = borsh::to_vec(&r).unwrap();
    let back: SignedEpochRecord = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, r);
}

#[test]
fn signed_epoch_check_outcome_borsh_roundtrip() {
    for o in [SignedEpochCheckOutcome::Allow, SignedEpochCheckOutcome::AllowRebroadcast, SignedEpochCheckOutcome::Block] {
        let bytes = borsh::to_vec(&o).unwrap();
        let back: SignedEpochCheckOutcome = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, o);
    }
    assert_eq!(SignedEpochCheckOutcome::default(), SignedEpochCheckOutcome::Allow);
}

// ---- check_signed_epoch_record — full decision matrix -----------

#[test]
fn equivocation_check_allow_when_no_prior() {
    let candidate = fixture_signed();
    assert_eq!(check_signed_epoch_record(None, &candidate), SignedEpochCheckOutcome::Allow);
}

#[test]
fn equivocation_check_allow_rebroadcast_when_exact_match() {
    let prev = fixture_signed();
    let candidate = prev.clone();
    assert_eq!(check_signed_epoch_record(Some(&prev), &candidate), SignedEpochCheckOutcome::AllowRebroadcast);
}

#[test]
fn equivocation_check_allow_rebroadcast_when_only_signature_fingerprint_differs() {
    // ML-DSA-87 is hedged by default (FIPS 204 §3.4); two
    // valid signatures over the same message differ on the
    // `rnd` parameter. Bit-equality on the fingerprint would
    // therefore be too strict, and would falsely block honest
    // re-signs after a restart. The predicate that matters is
    // (target_hash, target_daa_score) equality — this test
    // pins that.
    let prev = fixture_signed();
    let mut candidate = prev.clone();
    candidate.signature_fingerprint = Hash64::from_bytes([0xcdu8; 64]); // different fingerprint
    assert_eq!(check_signed_epoch_record(Some(&prev), &candidate), SignedEpochCheckOutcome::AllowRebroadcast);
}

#[test]
fn equivocation_check_block_when_target_hash_differs() {
    let prev = fixture_signed();
    let mut candidate = prev.clone();
    candidate.target_hash = Hash64::from_bytes([0x99u8; 64]); // different anchor — would be equivocation
    assert_eq!(check_signed_epoch_record(Some(&prev), &candidate), SignedEpochCheckOutcome::Block);
}

#[test]
fn equivocation_check_block_when_target_daa_score_differs() {
    let prev = fixture_signed();
    let mut candidate = prev.clone();
    // Same target_hash but different DAA score still counts —
    // ADR-0009 §"`SlashingEvidencePayload`" lists this as
    // evidence; the rare case of two attestations sharing
    // target_hash at different DAA scores is a node bug, and
    // signing both would still be slashable.
    candidate.target_daa_score = prev.target_daa_score + 1;
    assert_eq!(check_signed_epoch_record(Some(&prev), &candidate), SignedEpochCheckOutcome::Block);
}

#[test]
fn equivocation_check_block_when_both_target_fields_differ() {
    let prev = fixture_signed();
    let candidate = SignedEpochRecord {
        epoch: prev.epoch,
        target_hash: Hash64::from_bytes([0x99u8; 64]),
        target_daa_score: prev.target_daa_score + 1000,
        signature_fingerprint: Hash64::from_bytes([0xeeu8; 64]),
    };
    assert_eq!(check_signed_epoch_record(Some(&prev), &candidate), SignedEpochCheckOutcome::Block);
}

// ---- Reward / slashing distribution (ADR-0013) ----------------

#[test]
fn reward_params_borsh_roundtrip() {
    let p = RewardParams {
        per_attestation_reward_sompi: 100_000,
        slashing_reporter_reward_bps: 1000, // 10%
        max_validator_inflation_per_block_sompi: 5_000_000_000,
        validator_participation_bps: 10000,
        validator_quality_bonus_bps: 0,
        quality_gate_bonus_sompi: 25_000_000,
        worker_urgency_multiplier_scaled: STAKE_SCORE_SCALE as u64,
        fee_split: fixture_fee_split(),
        fee_split_bootstrap: fixture_fee_split_bootstrap(),
        security_reserve_bps: 2000,
        victim_epoch_pool_bps: 1000,
        reserve_drip_per_epoch_cap_sompi: 1_000_000,
    };
    let bytes = borsh::to_vec(&p).unwrap();
    let back: RewardParams = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, p);
}

#[test]
fn attestation_reward_payout_borsh_roundtrip() {
    let p = AttestationRewardPayout { total_payout_sompi: 1_600_000, refunded_sompi: 0 };
    let bytes = borsh::to_vec(&p).unwrap();
    let back: AttestationRewardPayout = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, p);
}

// ---- ADR-0018 §D/§E inclusion economics --------------------------

#[test]
fn worker_inclusion_bounty_is_proportional_and_anti_capture() {
    let scale = STAKE_SCORE_SCALE; // 1.0× urgency
    // Degenerate epoch (no expected stake) → 0.
    assert_eq!(worker_inclusion_bounty(1000, 5, 0, scale, false, 0), 0);
    // Proportional: 5/10 of a 1000 pool at 1.0× urgency, no cross → 500.
    assert_eq!(worker_inclusion_bounty(1000, 5, 10, scale, false, 0), 500);
    // Urgency 2.0× doubles the base.
    assert_eq!(worker_inclusion_bounty(1000, 5, 10, 2 * scale, false, 0), 1000);
    // Crossing φS adds the fixed quality-gate bonus on top of the urgent base...
    assert_eq!(worker_inclusion_bounty(1000, 5, 10, scale, true, 123), 500 + 123);
    // ...but only when this block actually crosses the floor.
    assert_eq!(worker_inclusion_bounty(1000, 5, 10, scale, false, 123), 500);
    // Anti-capture: a few attestations earn only a tiny slice (1/1000 of the pool),
    // NOT the whole pool (the rejected `pool / included_count` design).
    assert_eq!(worker_inclusion_bounty(1000, 1, 1000, scale, false, 0), 1);
    // Anti-drain: over-counted stake (> expected) clamps to the whole pool, never more.
    assert_eq!(worker_inclusion_bounty(1000, 9999, 10, scale, false, 0), 1000);
}

#[test]
fn split_validator_pool_is_dustfree() {
    // 70/30 split sums to the pool exactly.
    assert_eq!(split_validator_pool(1000, 7000), (700, 300));
    // Rounding dust lands in the bonus pool; the two still sum to the pool.
    let (p, q) = split_validator_pool(1001, 7000); // 1001 × 0.70 = 700.7 → 700
    assert_eq!((p, q), (700, 301));
    assert_eq!(p + q, 1001);
    // Edges: all-participation / all-bonus; bps clamps at 10_000.
    assert_eq!(split_validator_pool(1000, 10_000), (1000, 0));
    assert_eq!(split_validator_pool(1000, 0), (0, 1000));
    assert_eq!(split_validator_pool(1000, 20_000), (1000, 0)); // clamp
}

#[test]
fn validator_two_pool_reward_is_proportional_and_gated() {
    // Participation: 10/100 of a 1000 pool → 100, paid regardless of the floor.
    assert_eq!(validator_participation_reward(1000, 10, 100), 100);
    assert_eq!(validator_participation_reward(1000, 10, 0), 0); // expected == 0
    // Anti-capture: a minority validator earns only its proportional slice, not the pool.
    assert_eq!(validator_participation_reward(1000, 1, 1000), 1);
    // Quality bonus: same proportional share, but ONLY when the epoch met φS.
    assert_eq!(validator_quality_bonus(300, 10, 100, true), 30);
    assert_eq!(validator_quality_bonus(300, 10, 100, false), 0); // below φS → 0, pool rolls over
    assert_eq!(validator_quality_bonus(300, 10, 0, true), 0); // expected == 0
    // Combined per-validator payout = participation + (bonus iff floor met). A 1000 pool
    // split 70/30, validator holds 10% of expected stake → 70 + 30 above φS, 70 below.
    let (part_pool, bonus_pool) = split_validator_pool(1000, 7000);
    let combined = |met| validator_participation_reward(part_pool, 10, 100) + validator_quality_bonus(bonus_pool, 10, 100, met);
    assert_eq!(combined(true), 70 + 30);
    assert_eq!(combined(false), 70);
}

// ---- ADR-0018 §F fee / subsidy split -----------------------------

#[test]
fn fee_splits_are_dust_free_and_match_adr_ratios() {
    let p = fixture_fee_split();
    // Subsidy 67/8/25/0 on a round 1_000_000; Worker total = base + inclusion = 75%.
    let s = split_block_subsidy(1_000_000, &p);
    assert_eq!((s.worker_base_sompi, s.worker_inclusion_sompi, s.validator_sompi, s.service_sompi), (670_000, 80_000, 250_000, 0));
    assert_eq!(s.worker_base_sompi + s.worker_inclusion_sompi, 750_000); // Worker 75%
    // Dust-free: the four parts always sum to the input, even on a non-round value;
    // the primary (worker_base) absorbs the rounding remainder, so it is ≥ its nominal.
    let s = split_block_subsidy(1_000_003, &p);
    assert_eq!(s.worker_base_sompi + s.worker_inclusion_sompi + s.validator_sompi + s.service_sompi, 1_000_003);
    assert!(s.worker_base_sompi >= 670_001);

    // Normal-tx 90/10/0 (Worker primary).
    let n = split_normal_tx_fees(1_000_000, &p);
    assert_eq!((n.worker_sompi, n.validator_sompi, n.service_sompi), (900_000, 100_000, 0));
    let n = split_normal_tx_fees(777, &p);
    assert_eq!(n.worker_sompi + n.validator_sompi + n.service_sompi, 777); // dust-free

    // DNS-finality 75/25/0 (Validator primary; unwired).
    let f = split_finality_fees(1_000_000, &p);
    assert_eq!((f.validator_sompi, f.worker_sompi, f.service_sompi), (750_000, 250_000, 0));
    let f = split_finality_fees(333, &p);
    assert_eq!(f.worker_sompi + f.validator_sompi + f.service_sompi, 333); // dust-free

    // Zero input → all-zero (inert: no subsidy/fees → no split outputs).
    let z = split_block_subsidy(0, &p);
    assert_eq!((z.worker_base_sompi, z.worker_inclusion_sompi, z.validator_sompi, z.service_sompi), (0, 0, 0, 0));
}

#[test]
fn fee_split_params_partition_to_100_percent() {
    let p = fixture_fee_split();
    assert_eq!(p.subsidy_worker_base_bps + p.subsidy_worker_inclusion_bps + p.subsidy_validator_bps + p.subsidy_service_bps, 10_000);
    assert_eq!(p.normal_fee_worker_bps + p.normal_fee_validator_bps + p.normal_fee_service_bps, 10_000);
    assert_eq!(p.finality_fee_validator_bps + p.finality_fee_worker_bps + p.finality_fee_service_bps, 10_000);
}

#[test]
fn fee_split_types_borsh_roundtrip() {
    let fsp = fixture_fee_split();
    assert_eq!(borsh::from_slice::<FeeSplitParams>(&borsh::to_vec(&fsp).unwrap()).unwrap(), fsp);
    let s =
        SubsidySplit { worker_base_sompi: 620_000, worker_inclusion_sompi: 80_000, validator_sompi: 250_000, service_sompi: 50_000 };
    assert_eq!(borsh::from_slice::<SubsidySplit>(&borsh::to_vec(&s).unwrap()).unwrap(), s);
    let f = FeeSplit { worker_sompi: 850_000, validator_sompi: 100_000, service_sompi: 50_000 };
    assert_eq!(borsh::from_slice::<FeeSplit>(&borsh::to_vec(&f).unwrap()).unwrap(), f);
}

#[test]
fn split_block_reward_combines_subsidy_and_fee_splits() {
    let p = fixture_fee_split();
    // 1_000_000 subsidy (67+8/25/0) + 1_000_000 all-normal fees (90/10/0):
    // worker 670k+80k+900k = 1_650_000; validator 250k+100k = 350_000; service 0.
    let r = split_block_reward(1_000_000, 1_000_000, 0, &p);
    assert_eq!((r.worker_sompi, r.validator_sompi, r.service_sompi), (1_650_000, 350_000, 0));
    // Value-conserving: the three sum to subsidy + fees exactly.
    assert_eq!(r.worker_sompi + r.validator_sompi + r.service_sompi, 2_000_000);
    // No fees → just the subsidy split (75/25/0 of 1_000_000).
    let r = split_block_reward(1_000_000, 0, 0, &p);
    assert_eq!((r.worker_sompi, r.validator_sompi, r.service_sompi), (750_000, 250_000, 0));
}

/// ADR-0018 §F bridge wiring: the finality-class subset of a block's fees is split at the
/// validator-primary finality ratios (25/75/0) while the normal-class remainder keeps the
/// 90/10/0 normal ratios; `finality = 0` is byte-identical to the pre-wiring math; the
/// split conserves value and clamps a (construction-impossible) finality > total.
#[test]
fn split_block_reward_finality_class_pays_validator_primary() {
    let p = fixture_fee_split();
    // 1_000_000 subsidy + 1_000_000 total fees of which 400_000 finality-class:
    //   subsidy:   worker 670k+80k / validator 250k
    //   normal 600k (90/10): worker 540k / validator 60k
    //   finality 400k (25/75): worker 100k / validator 300k
    // ⇒ worker 1_390_000; validator 610_000; service 0.
    let r = split_block_reward(1_000_000, 1_000_000, 400_000, &p);
    assert_eq!((r.worker_sompi, r.validator_sompi, r.service_sompi), (1_390_000, 610_000, 0));
    assert_eq!(r.worker_sompi + r.validator_sompi + r.service_sompi, 2_000_000, "value-conserving");

    // ALL fees finality-class: worker 670k+80k+250k = 1_000_000; validator 250k+750k = 1_000_000.
    let r = split_block_reward(1_000_000, 1_000_000, 1_000_000, &p);
    assert_eq!((r.worker_sompi, r.validator_sompi, r.service_sompi), (1_000_000, 1_000_000, 0));

    // Fence-off equivalence: finality 0 == the historical two-class math (the pre-wiring shape).
    assert_eq!(split_block_reward(123_456, 789_012, 0, &p), {
        let s = split_block_subsidy(123_456, &p);
        let f = split_normal_tx_fees(789_012, &p);
        FeeSplit {
            worker_sompi: s.worker_base_sompi + s.worker_inclusion_sompi + f.worker_sompi,
            validator_sompi: s.validator_sompi + f.validator_sompi,
            service_sompi: s.service_sompi + f.service_sompi,
        }
    });

    // Defensive clamp: finality > total behaves as finality == total (cannot over-split).
    assert_eq!(split_block_reward(0, 1_000, 5_000, &p), split_block_reward(0, 1_000, 1_000, &p));
}

/// Defensive conservation: a (mis)configured bps group summing past 10_000 cannot
/// over-mint — every split still sums EXACTLY to its input (the sequential caps floor
/// the over-shares; the primary floors at 0). All current presets sum to exactly
/// 10_000, where the caps are no-ops (the exact-ratio tests above cover that).
#[test]
fn splits_conserve_value_under_malformed_bps() {
    let mut p = fixture_fee_split();
    p.finality_fee_worker_bps = 9_000;
    p.finality_fee_service_bps = 9_000; // worker+service "180%" — must cap, never over-mint
    let f = split_finality_fees(1_000_000, &p);
    assert_eq!(f.worker_sompi + f.validator_sompi + f.service_sompi, 1_000_000, "finality split conserves");
    assert_eq!((f.worker_sompi, f.service_sompi, f.validator_sompi), (900_000, 100_000, 0));

    p.subsidy_worker_inclusion_bps = 8_000;
    p.subsidy_validator_bps = 8_000;
    p.subsidy_service_bps = 8_000; // "240%"
    let s = split_block_subsidy(1_000_000, &p);
    assert_eq!(
        s.worker_base_sompi + s.worker_inclusion_sompi + s.validator_sompi + s.service_sompi,
        1_000_000,
        "subsidy split conserves"
    );

    p.normal_fee_validator_bps = 7_000;
    p.normal_fee_service_bps = 7_000; // "140%"
    let n = split_normal_tx_fees(1_000_000, &p);
    assert_eq!(n.worker_sompi + n.validator_sompi + n.service_sompi, 1_000_000, "normal split conserves");

    // And the combined per-block split conserves under the same malformed config.
    let r = split_block_reward(1_000_000, 1_000_000, 400_000, &p);
    assert_eq!(r.worker_sompi + r.validator_sompi + r.service_sompi, 2_000_000, "combined split conserves");
}

#[test]
fn validator_participation_outputs_distribute_proportionally() {
    let a = [0xa1u8; 64];
    let b = [0xb2u8; 64];
    // Pool 1000, expected 100. A holds 60 → 600, B holds 30 → 300; the 10 uncovered stake's
    // 100 is unspent → not minted (don't-mint rollover).
    let outs = validator_participation_outputs(1000, &[(a, 60), (b, 30)], 100);
    assert_eq!(outs.len(), 2);
    assert_eq!((outs[0].value, outs[1].value), (600, 300));
    assert_eq!(outs[0].script_public_key, p2pkh_mldsa87_spk(&a));
    assert!(outs.iter().map(|o| o.value).sum::<u64>() <= 1000); // never over-mints the pool
    // Zero-value shares (zero stake / empty set) and a degenerate expected_stake emit nothing.
    assert!(validator_participation_outputs(1000, &[([0xcc; 64], 0)], 100).is_empty());
    assert!(validator_participation_outputs(1000, &[], 100).is_empty());
    assert!(validator_participation_outputs(1000, &[(a, 60)], 0).is_empty());
}

#[test]
fn validator_participation_reward_outputs_dedup_and_cap() {
    let op1 = TransactionOutpoint::new(Hash64::from_bytes([0x11; 64]), 0);
    let op2 = TransactionOutpoint::new(Hash64::from_bytes([0x22; 64]), 0);
    let (a, b) = ([0xa1u8; 64], [0xb2u8; 64]);
    let empty = RewardedEpochSet::new();

    // Pool 1000, expected 100: op1/epoch5 stake 60 → 600; op2/epoch5 stake 30 → 300 (10 uncovered).
    let atts = vec![(op1, 5u64, a, 60u64), (op2, 5, b, 30)];
    let (outs, keys) = validator_participation_reward_outputs(1000, 100, &atts, &empty);
    assert_eq!(outs.iter().map(|o| o.value).collect::<Vec<_>>(), vec![600, 300]);
    assert_eq!(keys, vec![(op1, 5), (op2, 5)]);

    // Within-block dedup: a repeated (op1, epoch5) earns nothing the second time.
    let (outs, keys) = validator_participation_reward_outputs(1000, 100, &[(op1, 5, a, 60), (op1, 5, a, 60)], &empty);
    assert_eq!((outs.len(), keys), (1, vec![(op1, 5)]));

    // Cross-block dedup: a (bond, epoch) already rewarded on the prefix is skipped.
    let mut seen = RewardedEpochSet::new();
    seen.insert(op1, 5);
    let (outs, keys) = validator_participation_reward_outputs(1000, 100, &atts, &seen);
    assert_eq!((outs.iter().map(|o| o.value).collect::<Vec<_>>(), keys), (vec![300], vec![(op2, 5)]));

    // Whole-output pool cap: op1 in epochs 5 AND 6 (distinct keys), each 600 → 1200 > pool 1000.
    // The first fits; the second is dropped (break) and left unrewarded → Σ ≤ pool.
    let (outs, keys) = validator_participation_reward_outputs(1000, 100, &[(op1, 5, a, 60), (op1, 6, a, 60)], &empty);
    assert_eq!(outs.iter().map(|o| o.value).sum::<u64>(), 600);
    assert_eq!(keys, vec![(op1, 5)]); // (op1, 6) NOT marked rewarded — a later block may pay it

    // Degenerate denominator → no outputs.
    assert!(validator_participation_reward_outputs(1000, 0, &[(op1, 5, a, 60)], &empty).0.is_empty());

    // **The COUNT cap, which the pool cap is not.** `PALW_V2_MAX_VALIDATOR_PAYOUTS` is what the
    // coinbase's isolation bound reserves for this fan-out; nothing here respected it, so
    // enough validators bonding made every coinbase exceed `outputs_limit` and every node
    // reject every block, permanently. A large pool and small rewards is exactly the shape
    // that used to produce an unbounded number of outputs.
    let cap = crate::palw_state_v2::PALW_V2_MAX_VALIDATOR_PAYOUTS as usize;
    let many: Vec<_> = (0..(cap as u64 + 5))
        .map(|i| (TransactionOutpoint::new(Hash64::from_u64_word(0x900 + i), 0), 5u64, [0xc3u8; 64], 1u64))
        .collect();
    // Each attester holds 1 of an expected 1000, so every reward is a thousandth of the pool:
    // twenty-one of them fit comfortably, and only the COUNT cap can stop the fan-out.
    let (outs, keys) = validator_participation_reward_outputs(1_000_000, 1_000, &many, &empty);
    assert_eq!(outs.len(), cap, "the fan-out is capped at what the coinbase can carry");
    assert_eq!(keys.len(), cap, "and exactly the paid ones are marked, so the tail drains next block");
}

#[test]
fn total_active_stake_at_sums_only_active() {
    let op1 = TransactionOutpoint::new(Hash64::from_bytes([0x11; 64]), 0);
    let op2 = TransactionOutpoint::new(Hash64::from_bytes([0x22; 64]), 0);
    let mut active = fixture_bond_record(op1);
    active.amount = 100;
    active.activation_daa_score = 50;
    let mut pending = fixture_bond_record(op2);
    pending.amount = 200;
    pending.activation_daa_score = 1000; // not active until daa 1000
    let view = ActiveBondView::from_records([(op1, active), (op2, pending)]);
    assert_eq!(view.total_active_stake_at(500), 100); // only op1 active; op2 still pending
    assert_eq!(view.total_active_stake_at(2000), 300); // both active
}

#[test]
fn slashing_distribution_borsh_roundtrip() {
    let s = SlashingDistribution {
        reporter_reward_sompi: 100_000,
        security_reserve_sompi: 0,
        victim_epoch_pool_sompi: 0,
        burned_sompi: 900_000,
    };
    let bytes = borsh::to_vec(&s).unwrap();
    let back: SlashingDistribution = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, s);
}

// ---- compute_attestation_reward_payouts -----------------------

#[test]
fn attestation_payout_under_cap_pays_full_amount() {
    // 8 attestations × 100_000 sompi = 800_000 sompi, well
    // under the 5_000_000_000 cap.
    let r = compute_attestation_reward_payouts(100_000, 8, 5_000_000_000);
    assert_eq!(r.total_payout_sompi, 800_000);
    assert_eq!(r.refunded_sompi, 0);
}

#[test]
fn attestation_payout_at_exact_cap_pays_cap_no_refund() {
    // Boundary: per × count == cap.
    let r = compute_attestation_reward_payouts(1_000_000, 5, 5_000_000);
    assert_eq!(r.total_payout_sompi, 5_000_000);
    assert_eq!(r.refunded_sompi, 0);
}

#[test]
fn attestation_payout_over_cap_emits_refund() {
    // 10 × 1_000_000 = 10_000_000, but cap is 5_000_000.
    // Refund = 10_000_000 - 5_000_000 = 5_000_000.
    let r = compute_attestation_reward_payouts(1_000_000, 10, 5_000_000);
    assert_eq!(r.total_payout_sompi, 5_000_000);
    assert_eq!(r.refunded_sompi, 5_000_000);
}

#[test]
fn attestation_payout_zero_count_is_zero() {
    let r = compute_attestation_reward_payouts(100_000, 0, 5_000_000_000);
    assert_eq!(r.total_payout_sompi, 0);
    assert_eq!(r.refunded_sompi, 0);
}

#[test]
fn attestation_payout_zero_reward_is_zero() {
    // Defensive — a network with per_attestation_reward = 0
    // pays nothing regardless of count.
    let r = compute_attestation_reward_payouts(0, 100, 5_000_000_000);
    assert_eq!(r.total_payout_sompi, 0);
    assert_eq!(r.refunded_sompi, 0);
}

#[test]
fn attestation_payout_saturates_on_huge_inputs() {
    // Defensive saturation — a bogus `(u64::MAX, u64::MAX as usize)`
    // input must produce a defined value rather than panic.
    let r = compute_attestation_reward_payouts(u64::MAX, usize::MAX, 1_000_000);
    // Saturated multiplication then capped at 1_000_000.
    assert_eq!(r.total_payout_sompi, 1_000_000);
    // Refund overflows u64 in absolute terms; we documented
    // refund as u64 so it saturates — what matters is the
    // helper does not panic. The refund is the uncapped product
    // (`u64::MAX * usize::MAX`, saturating) minus the 1_000_000 cap,
    // truncated to u64 — assert it matches that exact computation.
    let expected_refund = (u64::MAX as u128).saturating_mul(usize::MAX as u128).saturating_sub(1_000_000) as u64;
    assert_eq!(r.refunded_sompi, expected_refund);
}

// ---- p2pkh_mldsa87_spk + validator_reward_outputs -------------

#[test]
fn p2pkh_mldsa87_spk_byte_layout() {
    // ADR-0019 §8: widened to OP_BLAKE2B_512 (0xc4) + OP_DATA64 (0x40)
    // over a 64-byte BLAKE2b-512 payload — total 69 bytes + spk version 0.
    let payload = [0x11u8; 64];
    let spk = p2pkh_mldsa87_spk(&payload);
    assert_eq!(spk.version(), 0);
    let script = spk.script();
    assert_eq!(script.len(), 69);
    assert_eq!(script[0], 0x76, "OpDup");
    assert_eq!(script[1], 0xc4, "OpBlake2b512");
    assert_eq!(script[2], 0x40, "OpData64");
    assert_eq!(&script[3..67], &payload, "64-byte payload");
    assert_eq!(script[67], 0x88, "OpEqualVerify");
    assert_eq!(script[68], 0xa6, "OpCheckSigMlDsa87");
}

#[test]
fn p2pkh_mldsa87_spk_distinct_payloads_distinct_scripts() {
    // The only varying region is script[3..67]; distinct payloads
    // must yield distinct scripts (no accidental collision).
    let a = p2pkh_mldsa87_spk(&[0x01u8; 64]);
    let b = p2pkh_mldsa87_spk(&[0x02u8; 64]);
    assert_ne!(a, b);
    // Same payload → identical script (deterministic).
    assert_eq!(p2pkh_mldsa87_spk(&[0x01u8; 64]), a);
}

#[test]
fn validator_reward_outputs_one_per_attestation() {
    let reward = 1_000_000u64;
    let cap = 100_000_000u64; // far above reward × count
    let payloads = [[0x01u8; 64], [0x02u8; 64], [0x03u8; 64]];
    let outs = validator_reward_outputs(reward, cap, &payloads);
    assert_eq!(outs.len(), payloads.len());
    for (out, p) in outs.iter().zip(payloads.iter()) {
        assert_eq!(out.value, reward);
        assert_eq!(out.script_public_key, p2pkh_mldsa87_spk(p));
    }
}

#[test]
fn validator_reward_outputs_preserves_canonical_order() {
    // Outputs must follow the caller's supplied order verbatim.
    let payloads = [[0x0au8; 64], [0x0bu8; 64], [0x0cu8; 64]];
    let outs = validator_reward_outputs(10, 1_000, &payloads);
    let got: Vec<_> = outs.iter().map(|o| o.script_public_key.clone()).collect();
    let want: Vec<_> = payloads.iter().map(p2pkh_mldsa87_spk).collect();
    assert_eq!(got, want);
}

#[test]
fn validator_reward_outputs_whole_output_cap_truncates_tail() {
    // cap = 25, reward = 10 → only 2 whole outputs (20), never a
    // partial third. Tail (3rd payload) is dropped.
    let reward = 10u64;
    let cap = 25u64;
    let payloads = [[0x01u8; 64], [0x02u8; 64], [0x03u8; 64]];
    let outs = validator_reward_outputs(reward, cap, &payloads);
    assert_eq!(outs.len(), 2);
    assert_eq!(outs.iter().map(|o| o.value).sum::<u64>(), 20);
    // The two emitted outputs are the canonical-order head.
    assert_eq!(outs[0].script_public_key, p2pkh_mldsa87_spk(&payloads[0]));
    assert_eq!(outs[1].script_public_key, p2pkh_mldsa87_spk(&payloads[1]));
}

#[test]
fn validator_reward_outputs_empty_when_reward_zero() {
    // reward = 0 → no validator-side outflow regardless of count.
    let payloads = [[0x01u8; 64], [0x02u8; 64]];
    assert!(validator_reward_outputs(0, 1_000_000, &payloads).is_empty());
}

#[test]
fn validator_reward_outputs_empty_when_no_payloads() {
    // No included attestations → empty validator side → coinbase
    // unchanged. This is the every-current-network case.
    assert!(validator_reward_outputs(1_000_000, 100_000_000, &[]).is_empty());
}

#[test]
fn validator_reward_outputs_duplicate_payloads_not_combined() {
    // ADR-0013 §"Coinbase fan-out": two attestations sharing an
    // owner payload emit two outputs, never one combined output.
    let reward = 5u64;
    let payloads = [[0x07u8; 64], [0x07u8; 64]];
    let outs = validator_reward_outputs(reward, 1_000, &payloads);
    assert_eq!(outs.len(), 2);
    assert_eq!(outs[0], outs[1]);
    assert_eq!(outs[0].value, reward);
}

// ---- validator_reward_outputs_from_attestations (within-block dedup) ----

fn op_n(b: u8) -> TransactionOutpoint {
    TransactionOutpoint::new(Hash64::from_bytes([b; 64]), 0)
}

#[test]
fn reward_from_attestations_one_per_distinct_bond_epoch() {
    let reward = 100u64;
    let atts = [(op_n(1), 5u64, [0x01u8; 64]), (op_n(2), 5u64, [0x02u8; 64]), (op_n(3), 7u64, [0x03u8; 64])];
    let (outs, keys) = validator_reward_outputs_from_attestations(reward, 10_000, &atts, &RewardedEpochSet::new());
    assert_eq!(outs.len(), 3);
    assert_eq!(outs[0].script_public_key, p2pkh_mldsa87_spk(&[0x01u8; 64]));
    assert_eq!(outs[2].script_public_key, p2pkh_mldsa87_spk(&[0x03u8; 64]));
    assert_eq!(keys, vec![(op_n(1), 5), (op_n(2), 5), (op_n(3), 7)]);
}

#[test]
fn reward_from_attestations_dedups_same_bond_epoch_first_wins() {
    // Same (bond, epoch) twice → one reward; the FIRST occurrence's
    // payload is used (canonical order preserved).
    let atts = [(op_n(1), 5u64, [0xaau8; 64]), (op_n(1), 5u64, [0xbbu8; 64])];
    let (outs, keys) = validator_reward_outputs_from_attestations(100, 10_000, &atts, &RewardedEpochSet::new());
    assert_eq!(outs.len(), 1);
    assert_eq!(outs[0].script_public_key, p2pkh_mldsa87_spk(&[0xaau8; 64]));
    assert_eq!(keys, vec![(op_n(1), 5)]);
}

#[test]
fn reward_from_attestations_same_bond_distinct_epochs_both_paid() {
    // ADR-0013: a validator with attestations across two epochs in one
    // block earns for each — dedup is per (bond, epoch), not per bond.
    let atts = [(op_n(1), 5u64, [0xaau8; 64]), (op_n(1), 6u64, [0xaau8; 64])];
    let (outs, _) = validator_reward_outputs_from_attestations(100, 10_000, &atts, &RewardedEpochSet::new());
    assert_eq!(outs.len(), 2);
}

#[test]
fn reward_from_attestations_cap_applies_after_dedup() {
    // 3 distinct (after dedup) but cap allows only 2 whole rewards.
    let atts =
        [(op_n(1), 1u64, [0x01u8; 64]), (op_n(1), 1u64, [0x01u8; 64]), (op_n(2), 1u64, [0x02u8; 64]), (op_n(3), 1u64, [0x03u8; 64])];
    // dedup → {(1,1),(2,1),(3,1)} = 3 payloads; cap 25 / reward 10 = 2.
    let (outs, keys) = validator_reward_outputs_from_attestations(10, 25, &atts, &RewardedEpochSet::new());
    assert_eq!(outs.len(), 2);
    assert_eq!(keys.len(), 2);
}

#[test]
fn reward_from_attestations_skips_already_rewarded_prefix() {
    // Cross-block uniqueness (§B.3(c)): a (bond, epoch) already rewarded on
    // the prefix earns nothing now; the rest are still paid.
    let mut prefix = RewardedEpochSet::new();
    prefix.insert(op_n(1), 5);
    let atts = [(op_n(1), 5u64, [0x01u8; 64]), (op_n(2), 5u64, [0x02u8; 64])];
    let (outs, keys) = validator_reward_outputs_from_attestations(100, 10_000, &atts, &prefix);
    assert_eq!(outs.len(), 1);
    assert_eq!(outs[0].script_public_key, p2pkh_mldsa87_spk(&[0x02u8; 64]));
    assert_eq!(keys, vec![(op_n(2), 5)]);
}

#[test]
fn reward_from_attestations_empty_is_empty() {
    let (outs, keys) = validator_reward_outputs_from_attestations(100, 10_000, &[], &RewardedEpochSet::new());
    assert!(outs.is_empty());
    assert!(keys.is_empty());
}

// ---- compute_slashing_distribution ----------------------------

#[test]
fn slashing_distribution_sums_to_slashed_amount() {
    // The invariant ADR-0013 §"Slashing distribution"
    // requires: no value created or destroyed by rounding.
    for slashed in [1u64, 100, 12345, 1_000_000_000, u64::MAX / 2] {
        for bps in [0u16, 1, 1000, 5000, 9999, 10000] {
            let d = compute_slashing_distribution(slashed, bps, 0, 0);
            assert_eq!(
                d.reporter_reward_sompi + d.security_reserve_sompi + d.victim_epoch_pool_sompi + d.burned_sompi,
                slashed,
                "slashed={slashed} bps={bps}"
            );
        }
    }
}

#[test]
fn slashing_distribution_zero_bps_burns_everything() {
    let d = compute_slashing_distribution(1_000_000_000, 0, 0, 0);
    assert_eq!(d.reporter_reward_sompi, 0);
    assert_eq!(d.burned_sompi, 1_000_000_000);
}

#[test]
fn slashing_distribution_full_bps_burns_nothing() {
    let d = compute_slashing_distribution(1_000_000_000, 10000, 0, 0);
    assert_eq!(d.reporter_reward_sompi, 1_000_000_000);
    assert_eq!(d.burned_sompi, 0);
}

#[test]
fn slashing_distribution_mainnet_10pct_recommendation() {
    // ADR-0013 §"Slashing distribution" mainnet recommendation:
    // 1000 bps = 10% to reporter, 90% burned.
    let d = compute_slashing_distribution(100_000_000_000, 1000, 0, 0);
    assert_eq!(d.reporter_reward_sompi, 10_000_000_000);
    assert_eq!(d.burned_sompi, 90_000_000_000);
}

#[test]
fn slashing_distribution_no_overflow_at_u64_max() {
    // u64::MAX × 10000 would overflow u64; the helper promotes
    // to u128 internally so it cannot. Pin this with the
    // largest plausible slashed amount (full u64 supply).
    let d = compute_slashing_distribution(u64::MAX, 10000, 0, 0);
    assert_eq!(d.reporter_reward_sompi, u64::MAX);
    assert_eq!(d.burned_sompi, 0);
}

// ---- ADR-0018 "本格版" (PoS-v2) Phase 3: 4-way slashing + victim compensation ----

/// 4-way split (reporter→reserve→victim→burn) sums to S; reserve/victim = 0 ⇒ byte-identical 2-way.
#[test]
fn slashing_distribution_four_way_splits_and_conserves() {
    // 10% reporter, 20% reserve, 30% victim, 40% burn of 1_000_000.
    let d = compute_slashing_distribution(1_000_000, 1000, 2000, 3000);
    assert_eq!(d.reporter_reward_sompi, 100_000);
    assert_eq!(d.security_reserve_sompi, 200_000);
    assert_eq!(d.victim_epoch_pool_sompi, 300_000);
    assert_eq!(d.burned_sompi, 400_000);
    assert_eq!(d.reporter_reward_sompi + d.security_reserve_sompi + d.victim_epoch_pool_sompi + d.burned_sompi, 1_000_000);
    // Fenced (reserve/victim bps = 0): exactly the pre-v2 reporter + burn.
    let two = compute_slashing_distribution(1_000_000, 1000, 0, 0);
    assert_eq!(
        (two.reporter_reward_sompi, two.security_reserve_sompi, two.victim_epoch_pool_sompi, two.burned_sompi),
        (100_000, 0, 0, 900_000)
    );
}

/// Misconfigured Σbps > 10_000: priority clamp (reporter→reserve→victim) keeps burn ≥ 0 and Σ = S.
#[test]
fn slashing_distribution_overallocated_bps_clamps_in_priority() {
    // 6000+6000+6000 = 18000 bps. reporter 600k, reserve min(600k, 400k)=400k, victim 0, burn 0.
    let d = compute_slashing_distribution(1_000_000, 6000, 6000, 6000);
    assert_eq!(
        (d.reporter_reward_sompi, d.security_reserve_sompi, d.victim_epoch_pool_sompi, d.burned_sompi),
        (600_000, 400_000, 0, 0)
    );
    assert_eq!(d.reporter_reward_sompi + d.security_reserve_sompi + d.victim_epoch_pool_sompi + d.burned_sompi, 1_000_000);
}

/// Victim compensation is stake-proportional over the honest set; Σ ≤ pool; degenerate ⇒ empty.
#[test]
fn victim_compensation_outputs_stake_proportional() {
    let honest = vec![(Hash64::from_bytes([0xA1; 64]), 100u64), (Hash64::from_bytes([0xB2; 64]), 300u64)];
    let out = victim_compensation_outputs(1000, &honest, 400);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].value, 250); // 1000 · 100/400
    assert_eq!(out[0].script_public_key, p2pkh_mldsa87_spk(&[0xA1; 64]));
    assert_eq!(out[1].value, 750); // 1000 · 300/400
    assert!(out.iter().map(|o| o.value as u128).sum::<u128>() <= 1000);
    // Zero pool / empty honest set / zero denominator ⇒ no outputs.
    assert!(victim_compensation_outputs(0, &honest, 400).is_empty());
    assert!(victim_compensation_outputs(1000, &[], 0).is_empty());
}

// ---- apply_unreveal_reporter_min_cap --------------------------

#[test]
fn unreveal_min_cap_clamps_when_bps_reward_exceeds_floor() {
    // bps-derived reporter = 1_000_000 (10% of 10_000_000),
    // floor = 500_000. After cap: reporter = 500_000, burned
    // grows by 500_000.
    let base = compute_slashing_distribution(10_000_000, 1000, 0, 0);
    assert_eq!(base.reporter_reward_sompi, 1_000_000);
    assert_eq!(base.burned_sompi, 9_000_000);

    let capped = apply_unreveal_reporter_min_cap(base, 500_000);
    assert_eq!(capped.reporter_reward_sompi, 500_000);
    assert_eq!(capped.burned_sompi, 9_500_000);
    // Invariant survives the cap.
    assert_eq!(capped.reporter_reward_sompi + capped.burned_sompi, 10_000_000);
}

#[test]
fn unreveal_min_cap_noop_when_bps_reward_under_floor() {
    // bps-derived reporter = 1_000 (10% of 10_000),
    // floor = 500_000. cap is a no-op.
    let base = compute_slashing_distribution(10_000, 1000, 0, 0);
    let capped = apply_unreveal_reporter_min_cap(base, 500_000);
    assert_eq!(capped, base);
}

#[test]
fn unreveal_min_cap_at_exact_floor_is_noop() {
    // bps-derived reporter == floor. No clamp applies.
    let base = compute_slashing_distribution(5_000_000, 1000, 0, 0); // → reporter = 500_000
    assert_eq!(base.reporter_reward_sompi, 500_000);
    let capped = apply_unreveal_reporter_min_cap(base, 500_000);
    assert_eq!(capped, base);
}

// ---- slashing_distribution_output (Addendum C) ----------------

#[test]
fn slashing_distribution_output_equivocation_pays_reporter_and_burns_rest() {
    // S = 1_000_000, bps = 1000 (10%) → reporter 100_000, burn 900_000.
    let payload = [0x5au8; 64];
    let (out, dist) = slashing_distribution_output(1_000_000, 1000, 0, 0, &payload, None);
    let out = out.expect("non-zero reporter reward");
    assert_eq!(out.value, 100_000);
    assert_eq!(out.script_public_key, p2pkh_mldsa87_spk(&payload));
    assert_eq!(dist.burned_sompi, 900_000);
    // Conservation: reporter + burn == S (reserve + victim are 0 in the 2-way case).
    assert_eq!(out.value + dist.burned_sompi, 1_000_000);
}

#[test]
fn slashing_distribution_output_zero_bps_emits_no_output() {
    // bps = 0 → everything burns, no reporter output.
    let (out, dist) = slashing_distribution_output(1_000_000, 0, 0, 0, &[0x5au8; 64], None);
    assert!(out.is_none());
    assert_eq!(dist.burned_sompi, 1_000_000);
}

#[test]
fn slashing_distribution_output_unreveal_applies_floor() {
    // S = 5_000_000, bps = 1000 → bps-reward 500_000, but the unreveal
    // floor caps the reporter at 100_000; the extra burns.
    let payload = [0x5au8; 64];
    let (out, dist) = slashing_distribution_output(5_000_000, 1000, 0, 0, &payload, Some(100_000));
    let out = out.expect("non-zero reporter reward");
    assert_eq!(out.value, 100_000);
    assert_eq!(dist.burned_sompi, 4_900_000);
    assert_eq!(out.value + dist.burned_sompi, 5_000_000);
}

// ---- slashing_side_effects_from_evidence (Addendum C / D.4) ----

fn slash_outpoint(b: u8) -> TransactionOutpoint {
    TransactionOutpoint::new(Hash64::from_bytes([b; 64]), 0)
}

#[test]
fn slashing_side_effects_distinct_bonds_pay_each_reporter() {
    // Two distinct bonds, bps = 1000 (10%): each yields one side-effect
    // with reporter = 10% and burn = 90% of its own S.
    let evidence = [
        (slash_outpoint(1), 1_000_000, [0x11u8; 64], Hash64::from_bytes([0xa1; 64]), 0),
        (slash_outpoint(2), 2_000_000, [0x22u8; 64], Hash64::from_bytes([0xa2; 64]), 0),
    ];
    let effects = slashing_side_effects_from_evidence(&evidence, 1000, 0, 0, None);
    assert_eq!(effects.len(), 2);

    assert_eq!(effects[0].bond_outpoint, slash_outpoint(1));
    assert_eq!(effects[0].slashed_amount_sompi, 1_000_000);
    assert_eq!(effects[0].reporter_output.as_ref().unwrap().value, 100_000);
    assert_eq!(effects[0].reporter_output.as_ref().unwrap().script_public_key, p2pkh_mldsa87_spk(&[0x11u8; 64]));
    assert_eq!(effects[0].burned_sompi, 900_000);
    // The reporter UTXO is minted at (slashing_tx_id, 0), so each effect
    // carries the id of the tx that fixes its mint outpoint.
    assert_eq!(effects[0].slashing_tx_id, Hash64::from_bytes([0xa1; 64]));

    assert_eq!(effects[1].bond_outpoint, slash_outpoint(2));
    assert_eq!(effects[1].reporter_output.as_ref().unwrap().value, 200_000);
    assert_eq!(effects[1].burned_sompi, 1_800_000);
    assert_eq!(effects[1].slashing_tx_id, Hash64::from_bytes([0xa2; 64]));
    // Conservation per bond: reporter + burn == S.
    assert_eq!(effects[1].reporter_output.as_ref().unwrap().value + effects[1].burned_sompi, 2_000_000);
}

#[test]
fn slashing_side_effects_dedup_same_bond_within_block() {
    // Two evidences against the SAME bond in one block collapse to a
    // single side-effect (its UTXO can be removed only once).
    let evidence = [
        (slash_outpoint(7), 1_000_000, [0x11u8; 64], Hash64::from_bytes([0xb1; 64]), 0),
        (slash_outpoint(7), 1_000_000, [0x99u8; 64], Hash64::from_bytes([0xb2; 64]), 0),
    ];
    let effects = slashing_side_effects_from_evidence(&evidence, 1000, 0, 0, None);
    assert_eq!(effects.len(), 1);
    // First occurrence wins (the 0x11 reporter payload) — and its tx id fixes
    // the mint outpoint (slashing_tx_id, 0), so a second tx for the same bond
    // can neither double-mint nor relocate the reporter UTXO.
    assert_eq!(effects[0].reporter_output.as_ref().unwrap().script_public_key, p2pkh_mldsa87_spk(&[0x11u8; 64]));
    assert_eq!(effects[0].slashing_tx_id, Hash64::from_bytes([0xb1; 64]));
}

#[test]
fn slashing_side_effects_zero_bps_burns_everything() {
    // bps = 0 → no reporter output, full S burns, but the bond is still
    // slashed (a side-effect is produced so its UTXO is removed).
    let evidence = [(slash_outpoint(3), 1_000_000, [0x33u8; 64], Hash64::from_bytes([0xc3; 64]), 0)];
    let effects = slashing_side_effects_from_evidence(&evidence, 0, 0, 0, None);
    assert_eq!(effects.len(), 1);
    assert!(effects[0].reporter_output.is_none());
    assert_eq!(effects[0].burned_sompi, 1_000_000);
    assert_eq!(effects[0].slashed_amount_sompi, 1_000_000);
}

#[test]
fn slashing_side_effects_unreveal_floor_caps_reporter() {
    // unreveal floor caps the reporter below the bps reward; the extra burns.
    let evidence = [(slash_outpoint(4), 5_000_000, [0x44u8; 64], Hash64::from_bytes([0xc4; 64]), 0)];
    let effects = slashing_side_effects_from_evidence(&evidence, 1000, 0, 0, Some(100_000));
    assert_eq!(effects[0].reporter_output.as_ref().unwrap().value, 100_000);
    assert_eq!(effects[0].burned_sompi, 4_900_000);
}

#[test]
fn slashing_side_effects_empty_is_noop() {
    assert!(slashing_side_effects_from_evidence(&[], 1000, 0, 0, None).is_empty());
}

// ---- resolve_slashing_side_effects (Addendum C / D.4): resolve a block's
// accepted evidence against its selected-parent bond view. Genuineness is a
// separate rule, so these only exercise the bond-status resolution.

// A slashing-evidence tx referencing `op` as its bond, paying `reporter`.
fn evidence_tx_for(op: TransactionOutpoint, reporter: [u8; 64]) -> Transaction {
    let ev = SlashingEvidencePayload {
        version: DNS_PAYLOAD_VERSION_V1,
        bond_outpoint: op,
        attestation_a: fixture_attestation(),
        attestation_b: fixture_attestation(),
        reporter_reward_spk_payload: reporter,
    };
    dns_overlay_tx(SUBNETWORK_ID_SLASHING_EVIDENCE, borsh::to_vec(&ev).unwrap())
}

// fixture_bond: amount = 100_000_000_000, activation = 5_000.
const RESOLVE_DAA: u64 = 10_000;

#[test]
fn resolve_slashing_side_effects_active_bond_yields_removal() {
    // Bond Active at the block DAA (activation 5_000 < 10_000) ⇒ one
    // side-effect, S = amount, reporter = 10% to the evidence's payload.
    let op = TransactionOutpoint::new(Hash64::from_bytes([0x11; 64]), 0);
    let view = ActiveBondView::from_records([(op, fixture_bond_record(op))]);
    let effects = resolve_slashing_side_effects(&[evidence_tx_for(op, [0xab; 64])], &view, RESOLVE_DAA, 1000, 0, 0);
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].bond_outpoint, op);
    assert_eq!(effects[0].slashed_amount_sompi, 100_000_000_000);
    assert_eq!(effects[0].reporter_output.as_ref().unwrap().value, 10_000_000_000);
    assert_eq!(effects[0].reporter_output.as_ref().unwrap().script_public_key, p2pkh_mldsa87_spk(&[0xab; 64]));
    assert_eq!(effects[0].burned_sompi, 90_000_000_000);
}

#[test]
fn resolve_slashing_side_effects_unbonding_bond_is_slashable() {
    // An Unbonding bond (unbond requested, far from release) still holds a
    // removable stake ⇒ slashable, so a validator cannot escape a slash by
    // unbonding first (ADR-0016 §D.3/§D.4).
    let op = TransactionOutpoint::new(Hash64::from_bytes([0x22; 64]), 0);
    let mut b = fixture_bond_record(op);
    b.unbond_request_daa_score = Some(RESOLVE_DAA - 1); // release = +unbonding_period ≫ DAA.
    assert_eq!(effective_bond_status(&b, RESOLVE_DAA), BondStatus::Unbonding);
    let view = ActiveBondView::from_records([(op, b)]);
    let effects = resolve_slashing_side_effects(&[evidence_tx_for(op, [0xcd; 64])], &view, RESOLVE_DAA, 1000, 0, 0);
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].slashed_amount_sompi, 100_000_000_000);
}

#[test]
fn resolve_slashing_side_effects_skips_pending_bond() {
    // A Pending bond (DAA below activation) is not yet slashable-for-
    // distribution ⇒ no side-effect.
    let op = TransactionOutpoint::new(Hash64::from_bytes([0x33; 64]), 0);
    let view = ActiveBondView::from_records([(op, fixture_bond_record(op))]);
    assert!(resolve_slashing_side_effects(&[evidence_tx_for(op, [0x01; 64])], &view, 1_000, 1000, 0, 0).is_empty());
}

#[test]
fn resolve_slashing_side_effects_skips_already_slashed_bond() {
    // An already-Slashed bond yields no side-effect: its stake was already
    // removed, so it is never removed twice (idempotent).
    let op = TransactionOutpoint::new(Hash64::from_bytes([0x44; 64]), 0);
    let mut b = fixture_bond_record(op);
    b.slashed_at_daa_score = Some(6_000);
    b.status = BondStatus::Slashed;
    let view = ActiveBondView::from_records([(op, b)]);
    assert!(resolve_slashing_side_effects(&[evidence_tx_for(op, [0x02; 64])], &view, RESOLVE_DAA, 1000, 0, 0).is_empty());
}

#[test]
fn resolve_slashing_side_effects_skips_unknown_bond() {
    // Evidence whose bond is absent from the view contributes nothing. (The
    // genuineness rule would already reject such a block.)
    let op = TransactionOutpoint::new(Hash64::from_bytes([0x55; 64]), 0);
    assert!(
        resolve_slashing_side_effects(&[evidence_tx_for(op, [0x03; 64])], &ActiveBondView::new(), RESOLVE_DAA, 1000, 0, 0).is_empty()
    );
}

#[test]
fn resolve_slashing_side_effects_empty_without_evidence() {
    let native = dns_overlay_tx(SubnetworkId::from_byte(0), vec![]);
    assert!(resolve_slashing_side_effects(&[native], &ActiveBondView::new(), RESOLVE_DAA, 1000, 0, 0).is_empty());
}

// ---- Coordinated-failover protocol (ADR-0014) -----------------

fn fixture_host_id(byte: u8) -> HostId {
    Hash::from_bytes([byte; 32])
}

#[test]
fn host_id_is_deterministic() {
    let nonce = [0x11u8; 32];
    let a = compute_host_id(b"primary.kaspa-pq.example.com", &nonce);
    let b = compute_host_id(b"primary.kaspa-pq.example.com", &nonce);
    assert_eq!(a, b);
}

#[test]
fn host_id_changes_with_hostname() {
    let nonce = [0x11u8; 32];
    let a = compute_host_id(b"primary.kaspa-pq.example.com", &nonce);
    let b = compute_host_id(b"standby.kaspa-pq.example.com", &nonce);
    assert_ne!(a, b);
}

#[test]
fn host_id_changes_with_nonce() {
    // Rebuilding the secondary host with a fresh nonce must
    // change its host_id — anti-spoofing rationale in ADR-0014
    // §"`host_id` derivation".
    let a = compute_host_id(b"primary.kaspa-pq.example.com", &[0x11u8; 32]);
    let b = compute_host_id(b"primary.kaspa-pq.example.com", &[0x12u8; 32]);
    assert_ne!(a, b);
}

#[test]
fn host_id_uses_host_id_key_domain() {
    // Hashing the same bytes with the generic (no-key)
    // BLAKE2b-256 yields a different value — pins the domain
    // separation. Defends against a future refactor accidentally
    // dropping the key.
    let nonce = [0x11u8; 32];
    let with_key = compute_host_id(b"hostname", &nonce);

    let mut without_key = Blake2bParams::new().hash_length(32).to_state();
    without_key.update(b"hostname");
    without_key.update(&nonce);
    let undomained = without_key.finalize();

    assert_ne!(with_key.as_bytes(), undomained.as_bytes());
}

// ---- Remote-signer protocol (ADR-0015) ------------------------

#[test]
fn signing_purpose_discriminants_are_api_stable() {
    // Wire-format discriminant; reordering is a protocol
    // hard fork. Pin to immediately trip drift.
    assert_eq!(SigningPurpose::Transaction as u8, 0);
    assert_eq!(SigningPurpose::Attestation as u8, 1);
    assert_eq!(SigningPurpose::TakeoverToken as u8, 2);
    assert_eq!(SigningPurpose::Unbond as u8, 3);
    // The PALW purposes were appended one by one; none was ever renumbered.
    assert_eq!(SigningPurpose::PalwAttemptV2 as u8, 4);
    assert_eq!(SigningPurpose::PalwFpCommitmentV3 as u8, 5);
    assert_eq!(SigningPurpose::PalwFpSpendV3 as u8, 6);
    assert_eq!(SigningPurpose::PalwDerivedArtifactV1 as u8, 7);
    // RFC-0009 stage C: the next explicit discriminant.
    assert_eq!(SigningPurpose::PalwReceiptAuthV4 as u8, 8);
}

/// The V4 authorization's digest variant carries its own purpose, and the wire bytes of every older variant are the ones they were:
/// the new digest is appended (borsh enum index 8) and a request for it round-trips.
#[test]
fn the_receipt_authorization_purpose_is_appended_and_its_digest_agrees_with_it() {
    let digest = SignerMessageDigest::PalwReceiptAuthV4(Hash64::from_bytes([0x44; 64]));
    assert_eq!(digest.purpose(), SigningPurpose::PalwReceiptAuthV4);
    let wire = borsh::to_vec(&digest).unwrap();
    assert_eq!(wire[0], 8, "an appended variant: the 9th, index 8");
    assert_eq!(borsh::from_slice::<SignerMessageDigest>(&wire).unwrap(), digest);
    // Older variants keep their indices.
    assert_eq!(borsh::to_vec(&SignerMessageDigest::Transaction(Hash64::from_bytes([1; 64]))).unwrap()[0], 0);
    assert_eq!(borsh::to_vec(&SignerMessageDigest::PalwDerivedArtifactV1(Hash64::from_bytes([1; 64]))).unwrap()[0], 7);
    let request = SignerRequest {
        request_id: 1,
        validator_id: Hash64::from_bytes([2; 64]),
        purpose: SigningPurpose::PalwReceiptAuthV4,
        context: crate::palw_receipt_v4::PALW_RECEIPT_V4_AUTH_MLDSA87_CONTEXT.to_vec(),
        message_digest: digest,
        metadata: SignerMetadata::None,
    };
    assert!(request.purpose_matches_digest());
    let mismatched = SignerRequest { purpose: SigningPurpose::PalwFpSpendV3, ..request.clone() };
    assert!(!mismatched.purpose_matches_digest(), "a tag that disagrees with the typed digest is malformed");
    assert_eq!(borsh::from_slice::<SignerRequest>(&borsh::to_vec(&request).unwrap()).unwrap(), request);
    // The context is the authorization's own: distinct from every other signing context of the lane.
    assert_ne!(
        crate::palw_receipt_v4::PALW_RECEIPT_V4_AUTH_MLDSA87_CONTEXT,
        crate::palw_receipt_v4::PALW_RECEIPT_V4_SPEND_MLDSA87_CONTEXT
    );
    assert_ne!(
        crate::palw_receipt_v4::PALW_RECEIPT_V4_AUTH_MLDSA87_CONTEXT,
        crate::palw_freeprompt_v3::PALW_FP_V3_MLDSA87_SPEND_CONTEXT
    );
}

#[test]
fn signing_purpose_default_is_transaction() {
    // Conservative default — `Transaction` is the original
    // ML-DSA-87 use site (ADR-0002), pre-DNS-overlay.
    assert_eq!(SigningPurpose::default(), SigningPurpose::Transaction);
}

#[test]
fn signing_purpose_borsh_roundtrip() {
    for p in [
        SigningPurpose::Transaction,
        SigningPurpose::Attestation,
        SigningPurpose::TakeoverToken,
        SigningPurpose::Unbond,
        SigningPurpose::PalwReceiptAuthV4,
    ] {
        let bytes = borsh::to_vec(&p).unwrap();
        let back: SigningPurpose = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, p);
        assert_eq!(bytes.len(), 1);
    }
}

#[test]
fn signer_policy_discriminants_are_api_stable() {
    assert_eq!(SignerPolicy::Permissive as u8, 0);
    assert_eq!(SignerPolicy::AuditOnly as u8, 1);
    assert_eq!(SignerPolicy::Strict as u8, 2);
}

#[test]
fn signer_policy_default_is_permissive() {
    // Matches the ADR-0010 local-key-file behaviour, so a
    // signer with no policy configured behaves like the
    // pre-ADR-0015 baseline.
    assert_eq!(SignerPolicy::default(), SignerPolicy::Permissive);
}

#[test]
fn signer_policy_borsh_roundtrip() {
    for p in [SignerPolicy::Permissive, SignerPolicy::AuditOnly, SignerPolicy::Strict] {
        let bytes = borsh::to_vec(&p).unwrap();
        let back: SignerPolicy = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, p);
    }
}

#[test]
fn signer_error_borsh_roundtrip_all_variants() {
    for e in [
        SignerError::ProtocolVersionMismatch,
        SignerError::KeyNotFound,
        SignerError::UnknownPurpose,
        SignerError::PolicyViolation("equivocation: target_hash differs".into()),
        SignerError::HsmError(0xCAFE_BABE, "CKR_DEVICE_ERROR".into()),
        SignerError::RateLimit,
        SignerError::InternalError("disk full".into()),
    ] {
        let bytes = borsh::to_vec(&e).unwrap();
        let back: SignerError = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, e);
    }
}

#[test]
fn signer_metadata_borsh_roundtrip_all_variants() {
    let none = SignerMetadata::None;
    let att = SignerMetadata::Attestation { epoch: 42, target_hash: Hash64::from_bytes([0x11u8; 64]), target_daa_score: 100 };
    let tk = SignerMetadata::TakeoverToken {
        yielding_host_id: fixture_host_id(0xa1),
        taking_over_host_id: fixture_host_id(0xa2),
        valid_from_epoch: 12345,
        grace_epochs: 1,
    };
    for m in [none, att, tk] {
        let bytes = borsh::to_vec(&m).unwrap();
        let back: SignerMetadata = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, m);
    }
}

#[test]
fn signer_hello_borsh_roundtrip() {
    let h = SignerHello {
        protocol_version: SIGNER_PROTOCOL_VERSION,
        capabilities: CAP_SIGN_ATTESTATION | CAP_POLICY_STRICT | CAP_AUDIT_LOG,
        client_identity: fixture_host_id(0xa1),
    };
    let bytes = borsh::to_vec(&h).unwrap();
    let back: SignerHello = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, h);
}

#[test]
fn signer_hello_ack_borsh_roundtrip() {
    let h = SignerHelloAck {
        protocol_version: SIGNER_PROTOCOL_VERSION,
        capabilities: CAP_SIGN_TRANSACTION | CAP_SIGN_ATTESTATION | CAP_SIGN_TAKEOVER_TOKEN | CAP_HSM_BACKED,
        server_identity: fixture_host_id(0xb1),
    };
    let bytes = borsh::to_vec(&h).unwrap();
    let back: SignerHelloAck = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, h);
}

fn fixture_signer_request() -> SignerRequest {
    SignerRequest {
        request_id: 7,
        validator_id: Hash64::from_bytes([0x42u8; 64]),
        purpose: SigningPurpose::Attestation,
        context: ATTESTATION_MLDSA87_CONTEXT.to_vec(),
        message_digest: SignerMessageDigest::Attestation(Hash::from_bytes([0xcdu8; 32])),
        metadata: SignerMetadata::Attestation { epoch: 42, target_hash: Hash64::from_bytes([0x11u8; 64]), target_daa_score: 100 },
    }
}

#[test]
fn signer_request_borsh_roundtrip() {
    let r = fixture_signer_request();
    let bytes = borsh::to_vec(&r).unwrap();
    let back: SignerRequest = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, r);
}

#[test]
fn signer_request_purpose_matches_typed_digest() {
    // audit H-03: the typed digest's purpose must agree with the request purpose, and a
    // transaction request carries a 64-byte sighash (unrepresentable under the old 32-byte field).
    assert!(fixture_signer_request().purpose_matches_digest()); // Attestation + Attestation
    let tx = SignerRequest {
        purpose: SigningPurpose::Transaction,
        message_digest: SignerMessageDigest::Transaction(Hash64::from_bytes([0x09u8; 64])),
        ..fixture_signer_request()
    };
    assert!(tx.purpose_matches_digest());
    // Transaction tag + Attestation digest (from the fixture) => mismatch, must be refused.
    let mismatched = SignerRequest { purpose: SigningPurpose::Transaction, ..fixture_signer_request() };
    assert!(!mismatched.purpose_matches_digest());
    assert_eq!(SignerMessageDigest::Unbond(Hash::from_bytes([0x1u8; 32])).purpose(), SigningPurpose::Unbond);
}

#[test]
fn signer_response_borsh_roundtrip_ok() {
    let r = SignerResponse { request_id: 7, result: Ok(vec![0xccu8; STAKE_ATTESTATION_SIG_LEN]) };
    let bytes = borsh::to_vec(&r).unwrap();
    let back: SignerResponse = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, r);
}

#[test]
fn signer_response_borsh_roundtrip_err() {
    let r = SignerResponse {
        request_id: 7,
        result: Err(SignerError::PolicyViolation("equivocation: target differs from epoch 42 record".into())),
    };
    let bytes = borsh::to_vec(&r).unwrap();
    let back: SignerResponse = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, r);
}

fn fixture_audit_record(outcome: SignerOutcome, sig_fingerprint: Hash64) -> SignerAuditRecord {
    SignerAuditRecord {
        timestamp_unix_secs: 1_700_000_000,
        client_identity: fixture_host_id(0xa1),
        request_id: 7,
        validator_id: Hash64::from_bytes([0x42u8; 64]),
        purpose: SigningPurpose::Attestation,
        metadata: SignerMetadata::Attestation { epoch: 42, target_hash: Hash64::from_bytes([0x11u8; 64]), target_daa_score: 100 },
        message_digest: SignerMessageDigest::Attestation(Hash::from_bytes([0xcdu8; 32])),
        signature_fingerprint: sig_fingerprint,
        outcome,
    }
}

#[test]
fn signer_audit_record_borsh_roundtrip_signed() {
    let r = fixture_audit_record(SignerOutcome::Signed, Hash64::from_bytes([0xeeu8; 64]));
    let bytes = borsh::to_vec(&r).unwrap();
    let back: SignerAuditRecord = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, r);
}

#[test]
fn signer_audit_record_borsh_roundtrip_refused() {
    let r = fixture_audit_record(SignerOutcome::Refused(SignerError::RateLimit), kaspa_hashes::ZERO_HASH64);
    let bytes = borsh::to_vec(&r).unwrap();
    let back: SignerAuditRecord = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, r);
}

#[test]
fn audit_chain_entry_is_deterministic() {
    let prev = Hash64::from_bytes([0x33u8; 64]);
    let rec = fixture_audit_record(SignerOutcome::Signed, Hash64::from_bytes([0xeeu8; 64]));
    let a = compute_signer_audit_chain_entry(prev, &rec);
    let b = compute_signer_audit_chain_entry(prev, &rec);
    assert_eq!(a, b);
}

#[test]
fn audit_chain_entry_changes_with_prev_hash() {
    let rec = fixture_audit_record(SignerOutcome::Signed, Hash64::from_bytes([0xeeu8; 64]));
    let a = compute_signer_audit_chain_entry(Hash64::from_bytes([0x33u8; 64]), &rec);
    let b = compute_signer_audit_chain_entry(Hash64::from_bytes([0x34u8; 64]), &rec);
    assert_ne!(a, b);
}

#[test]
fn audit_chain_entry_changes_with_record_content() {
    let prev = Hash64::from_bytes([0x33u8; 64]);
    let a = compute_signer_audit_chain_entry(prev, &fixture_audit_record(SignerOutcome::Signed, Hash64::from_bytes([0xeeu8; 64])));
    let b = compute_signer_audit_chain_entry(prev, &fixture_audit_record(SignerOutcome::Signed, Hash64::from_bytes([0xefu8; 64])));
    assert_ne!(a, b);
}

#[test]
fn audit_chain_walks_three_records_consistently() {
    // ADR-0015 §"Audit log" promises that walking the chain
    // from a known-good genesis hash deterministically
    // produces the same terminal hash for the same record
    // sequence. Pin a 3-record walk to verify the chaining
    // discipline.
    let genesis = kaspa_hashes::ZERO_HASH64;
    let r1 = fixture_audit_record(SignerOutcome::Signed, Hash64::from_bytes([0xe1u8; 64]));
    let r2 = fixture_audit_record(SignerOutcome::Refused(SignerError::RateLimit), kaspa_hashes::ZERO_HASH64);
    let r3 = fixture_audit_record(SignerOutcome::Signed, Hash64::from_bytes([0xe3u8; 64]));

    let h1 = compute_signer_audit_chain_entry(genesis, &r1);
    let h2 = compute_signer_audit_chain_entry(h1, &r2);
    let h3 = compute_signer_audit_chain_entry(h2, &r3);

    // Re-walking the same sequence must produce the same
    // terminal hash (chain replay).
    let h1b = compute_signer_audit_chain_entry(genesis, &r1);
    let h2b = compute_signer_audit_chain_entry(h1b, &r2);
    let h3b = compute_signer_audit_chain_entry(h2b, &r3);
    assert_eq!(h3, h3b);
}

#[test]
fn audit_chain_detects_record_insertion() {
    // Inserting a record between r1 and r2 must shift every
    // subsequent chain hash — this is the cryptographic
    // tamper-detection property.
    let genesis = kaspa_hashes::ZERO_HASH64;
    let r1 = fixture_audit_record(SignerOutcome::Signed, Hash64::from_bytes([0xe1u8; 64]));
    let r_evil = fixture_audit_record(SignerOutcome::Signed, Hash64::from_bytes([0xe9u8; 64]));
    let r2 = fixture_audit_record(SignerOutcome::Signed, Hash64::from_bytes([0xe2u8; 64]));

    let h1 = compute_signer_audit_chain_entry(genesis, &r1);
    let h_evil = compute_signer_audit_chain_entry(h1, &r_evil);
    let h2_after_insert = compute_signer_audit_chain_entry(h_evil, &r2);

    let h2_clean = compute_signer_audit_chain_entry(h1, &r2);
    assert_ne!(h2_after_insert, h2_clean, "post-insertion chain hash must differ from clean chain hash");
}

// ---- PR-10.5: the former `*_stub` panics are now implemented ----
// `compute_stake_score` / `check_dns_reorg_rule` replace the PR-10.3
// `unimplemented!()` stubs; behaviour is covered by the StakeScore +
// reorg-gate tests above.
