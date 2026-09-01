//! **The three series ADR-0038 Decision D is judged on, per epoch: expected, observed, target.**
//!
//! Testnet-11 cannot answer this today — it runs one class at the whole 1000‰, where the
//! retarget's expectation is realized production and the loop is an exact no-op by construction.
//! So the question "does a second class's difficulty adapt to it" is asked here, against the
//! REAL state machine (`apply_palw_transition_v2` — the same function block validation folds),
//! with testnet-11's own constants, and a second class registered beside the floor.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_class_daa_epoch_table -- --nocapture`

use kaspa_consensus_core::palw_attempt_v2::{PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2 as Obj, PalwPwuRuleV2, PalwStateParamsV2,
    apply_palw_transition_v2,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;

// testnet-11's shipped constants (`palw_fp_devnet_v3`), so the numbers below are the live
// network's arithmetic and not a fixture's.
const EPOCH_LENGTH: u64 = 1_000;
const MAX_FACTOR: u32 = 4;
const TOLERANCE_PERMILLE: u32 = 1_000;
const WINDOW_BIND: u64 = 600;
const BASE_PWU_PER_INFERENCE: u64 = 7_900; // t11's registered count: pwu 15800 at the genesis target
const QWEN_PWU_PER_INFERENCE: u64 = 7_900; // held equal on purpose — see `pwu_is_not_a_cross_class_price`
const GENESIS_TARGET: u128 = u128::MAX / 2;

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn params(base: Hash64) -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, WINDOW_BIND, 600, 1_200, 2_400, EPOCH_LENGTH, base, MAX_FACTOR, TOLERANCE_PERMILLE, 1, 1_000, 100)
        .expect("t11's own state params")
        .with_worker_carve_permille(620)
        .expect("carve")
        .with_turn_deadline_daa(1_200)
        .expect("turn deadline")
        .with_claim_retirement_daa(200)
        .expect("retirement")
}

fn bond_key() -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0))
}

fn registration(class: Hash64, share: u16, pwu_per_inference: u64) -> Obj {
    Obj::ClassRegistered {
        class_id: class,
        artifact_root: h(0xA7),
        slash_value_per_pwu: 1,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference },
        initial_target: GENESIS_TARGET,
        share_permille: share,
        activation_daa: 0,
        admission: None,
    }
}

fn attempt_from(bond: TransactionOutpoint, class: Hash64, pwu: u64, nonce: u64) -> PalwAttemptEnvelopeV2 {
    PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version: kaspa_consensus_core::palw_attempt_v2::PALW_ATTEMPT_V2_VERSION,
            network_domain: h(0xD0),
            challenge: h(nonce ^ 0xC0FFEE),
            class_id: class,
            executor_bond: bond,
            executor_pubkey: vec![7u8; 32],
            operator_id: kaspa_consensus_core::palw_state_v2::palw_operator_id_v2(&[21u8; 8]),
            artifact_root: h(0xA7),
            trace_root: h(nonce ^ 0x7A),
            output_root: h(nonce ^ 0x00FF),
            pwu,
            trace_manifest_root: h(0xD0),
            trace_chunk_count: 8,
            trace_retention_daa: u64::MAX,
            execution_root: h(nonce ^ 0x4E),
        },
        signature: vec![9u8; 64],
    }
}

struct Chain {
    state: PalwChainStateV2,
    params: PalwStateParamsV2,
    daa: u64,
    produced: std::collections::BTreeMap<Hash64, u64>,
    /// The bond every attempt binds to — the harness's own in the scenario sweep, and a real
    /// premine outpoint on the RC assembly, whose registry is the genesis card's.
    bond: TransactionOutpoint,
}

impl Chain {
    fn step(&mut self, class: Option<Hash64>, objects: &[Obj]) {
        self.daa += 1;
        let ctx = PalwBlockContextV2 { block: h(self.daa | 0x1000_0000), daa_score: self.daa, blue_score: self.daa, subsidy: 0 };
        let envelope = class.map(|c| {
            // What a producer must claim: the class's CURRENT target, from state.
            let target = self.state.class_target(&c).expect("the class holds a target").target;
            let per = match self.state.class(&c).expect("registered").pwu_rule {
                PalwPwuRuleV2::DerivedV1 { pwu_per_inference } => pwu_per_inference,
                PalwPwuRuleV2::MaxPerAttempt(cap) => cap,
            };
            attempt_from(self.bond, c, kaspa_consensus_core::palw_pwu::palw_pwu_v1(target, per), self.daa)
        });
        let (next, _) = apply_palw_transition_v2(&self.state, &self.params, &ctx, objects, envelope.as_ref())
            .unwrap_or_else(|e| panic!("block at daa {} refused: {e}", self.daa));
        self.state = next;
        if let Some(c) = class {
            *self.produced.entry(c).or_default() += 1;
        }
    }
}

/// `target / 2^128` — the readable form of a u128 target, and what one attempt's odds are.
fn odds(target: u128) -> f64 {
    (target as f64 + 1.0) / (u128::MAX as f64 + 1.0)
}

/// What the retarget expects of a class over a closed span: its share of REALIZED production.
fn expected_blocks(share: u16, realized_total: u64) -> u64 {
    (realized_total * share as u64 + 500) / 1000
}

struct Scenario {
    name: &'static str,
    qwen_blocks_per_epoch: u64,
    /// `None` = the minimum grantable share (what a post-genesis entrant is forced to take).
    qwen_share_permille: Option<u16>,
}

/// **The share the retarget actually uses** (audit H1): the table share renormalized over the
/// classes that PRODUCED in the span. A sole producer renormalizes to the whole denominator, which
/// is why a single-class chain is an exact no-op no matter what the table says.
fn effective_share(table_share: u16, competing_permille: u64) -> u16 {
    if competing_permille == 0 {
        return 0;
    }
    ((table_share as u64 * 1000 / competing_permille).min(1000)) as u16
}

fn run(scenario: &Scenario, epochs: u64) {
    let base = h(0xBA5E);
    let qwen = h(0x9E4);
    let p = params(base);
    let floor = scenario.qwen_share_permille.unwrap_or_else(|| p.min_grantable_share_permille());
    let mut chain = Chain { state: PalwChainStateV2::genesis(), params: p, daa: 0, produced: Default::default(), bond: bond_key().0 };

    // Block 1 carries the genesis object list: the bond every claim binds to, the floor at the
    // whole table, and the entrant funded by donation from it.
    let genesis_objects = vec![
        Obj::BondRegistered {
            bond: bond_key(),
            pubkey: vec![7u8; 32],
            operator_pubkey: vec![21u8; 8],
            collateral: 1_000_000_000_000_000,
            payout_payload: h(0x9A4),
            capable_classes: Default::default(),
            signature: vec![9u8; 64],
        },
        registration(base, 1_000, BASE_PWU_PER_INFERENCE),
        registration(qwen, floor, QWEN_PWU_PER_INFERENCE),
    ];
    chain.step(None, &genesis_objects);

    println!();
    println!("=== scenario: {} (entrant share {}‰) ===", scenario.name, floor);
    println!(
        "{:>5} {:>6} {:>6} {:>10} {:>9} {:>9} {:>7} {:>14} {:>14} {:>8}",
        "epoch", "class", "share", "share_eff", "expected", "observed", "budget", "target before", "target after", "moved"
    );

    for _ in 0..epochs {
        let mut qwen_made = 0u64;
        // One block per DAA unit — testnet-11's measured cadence (1000 blocks per 1000 DAA).
        loop {
            let class = if qwen_made < scenario.qwen_blocks_per_epoch {
                qwen_made += 1;
                qwen
            } else {
                base
            };
            chain.step(Some(class), &[]);
            if (chain.daa + 1).is_multiple_of(EPOCH_LENGTH) {
                break;
            }
        }
        // The closed epoch's facts, read before the boundary block folds them away.
        let closed = chain.daa / EPOCH_LENGTH;
        let counters: Vec<u64> = [base, qwen]
            .iter()
            .map(|c| chain.state.epoch_counter(c).filter(|k| k.epoch_index == closed).map(|k| k.produced_blocks).unwrap_or(0))
            .collect();
        let realized: u64 = counters.iter().sum();
        let before: Vec<(Hash64, u128, u16, u64)> = [base, qwen]
            .iter()
            .map(|c| {
                (
                    *c,
                    chain.state.class_target(c).unwrap().target,
                    chain.state.class_share_permille(c).unwrap_or(0),
                    chain.state.epoch_budgets().and_then(|b| b.budget_blocks.get(c).copied()).unwrap_or(0),
                )
            })
            .collect();
        // Crossing the boundary is what closes the span and runs the retarget.
        chain.step(Some(base), &[]);
        let competing: u64 =
            before.iter().enumerate().filter(|(i, _)| counters[*i] > 0).map(|(_, (_, _, share, _))| *share as u64).sum();
        for (idx, (class, target_before, share, budget)) in before.iter().enumerate() {
            let after = chain.state.class_target(class).unwrap().target;
            let eff = if counters[idx] > 0 { effective_share(*share, competing) } else { 0 };
            println!(
                "{:>5} {:>6} {:>5}‰ {:>9}‰ {:>9} {:>9} {:>7} {:>14.6} {:>14.6} {:>7.3}x",
                closed,
                if *class == base { "BASE" } else { "QWEN" },
                share,
                eff,
                expected_blocks(eff, realized),
                counters[idx],
                budget,
                odds(*target_before),
                odds(after),
                odds(after) / odds(*target_before)
            );
        }
    }
}

#[test]
fn per_epoch_expected_observed_target_for_a_second_class() {
    // A: the live shape — the entrant holds a permille and produces nothing.
    run(&Scenario { name: "entrant produces NOTHING", qwen_blocks_per_epoch: 0, qwen_share_permille: None }, 5);
    // B: the entrant produces exactly its share of the cadence.
    run(&Scenario { name: "entrant produces exactly its expectation", qwen_blocks_per_epoch: 1, qwen_share_permille: None }, 5);
    // C: the entrant floods — five times its share.
    run(&Scenario { name: "entrant produces 5x its expectation", qwen_blocks_per_epoch: 5, qwen_share_permille: None }, 5);
    // D: a class holding real cadence that produces far under it — the case a slow model is in.
    run(
        &Scenario {
            name: "entrant holds 100‰ and produces 1 block/epoch", qwen_blocks_per_epoch: 1, qwen_share_permille: Some(100)
        },
        6,
    );
    // E: the same class producing half its expectation.
    run(
        &Scenario {
            name: "entrant holds 100‰ and produces half its expectation",
            qwen_blocks_per_epoch: 50,
            qwen_share_permille: Some(100),
        },
        6,
    );
}

/// **The same three series for the REAL `PALW-QWEN36` class, over the RC genesis assembly.**
///
/// `palw_rc_params_with_qwen36` builds the two-class network: BASE-0 as the liveness floor and
/// Qwen3.6 registered beside it at the minimum grantable share, with the class's own counted
/// `pwu_per_inference` and the floor's target (ADR-0049 Decision H). This drives that genesis
/// object list through the transition for several epochs, so the entrant's expected / observed /
/// target are the real class's numbers rather than a fixture's.
///
/// The block plumbing (templates, signatures, acceptance) is the `kaspa-consensus` test's; what
/// this adds is speed — the epoch budget is asserted here rather than measured through admission.
#[test]
fn per_epoch_series_for_the_real_qwen36_class() {
    use kaspa_consensus_core::config::params::palw_rc_params_with_qwen36;
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

    let registry: Vec<_> = (0..kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1() as u32)
        .map(|i| kaspa_consensus_core::palw_fp_devnet_v3::PalwGenesisBondSpecV1 {
            bond: PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(i)),
            pubkey: vec![7u8.wrapping_add(i as u8); 32],
            operator_pubkey: vec![21u8, i as u8, 0, 0, 0, 0, 0, 0],
            payout_payload: h(0x9A11 + i as u64),
        })
        .collect();
    let params = palw_rc_params_with_qwen36(h(0xB0), h(0x93), registry).expect("the two-class genesis assembles");
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("a ConsensusV2 network") };
    let p = bundle.state.clone();
    let epoch_length = p.epoch_length();
    let mut ids: Vec<(Hash64, u64)> = Vec::new();
    for object in &bundle.genesis_objects {
        if let Obj::ClassRegistered { class_id, pwu_rule, .. } = object {
            let per = match pwu_rule {
                PalwPwuRuleV2::DerivedV1 { pwu_per_inference } => *pwu_per_inference,
                PalwPwuRuleV2::MaxPerAttempt(cap) => *cap,
            };
            ids.push((*class_id, per));
        }
    }
    assert_eq!(ids.len(), 2, "the floor and the entrant");
    let (base, base_per) = ids[0];
    let (qwen, qwen_per) = ids[1];
    println!();
    println!("=== the RC two-class assembly ===");
    println!("BASE-0  class {} pwu/inference {}", &base.to_string()[..16], base_per);
    println!("QWEN36  class {} pwu/inference {} ({}x the floor)", &qwen.to_string()[..16], qwen_per, qwen_per / base_per.max(1));
    println!("epoch_length {epoch_length}, entrant share {}‰", p.min_grantable_share_permille());

    let mut chain = Chain {
        state: PalwChainStateV2::genesis(),
        params: p.clone(),
        daa: 0,
        produced: Default::default(),
        bond: kaspa_consensus_core::config::premine::premine_outpoint(0),
    };
    chain.step(None, &bundle.genesis_objects);
    println!(
        "{:>5} {:>7} {:>6} {:>9} {:>9} {:>7} {:>12} {:>14} {:>14}",
        "epoch", "class", "share", "expected", "observed", "budget", "pwu", "target before", "target after"
    );
    // Epoch 0: the entrant is silent. From epoch 1 it produces exactly the one block its share and
    // its budget both allow — the only two states a minimum-share entrant can be in.
    for epoch in 0..4u64 {
        let quota = if epoch == 0 { 0 } else { 1 };
        let mut made = 0u64;
        loop {
            let class = if made < quota {
                made += 1;
                qwen
            } else {
                base
            };
            chain.step(Some(class), &[]);
            if (chain.daa + 1).is_multiple_of(epoch_length) {
                break;
            }
        }
        let closed = chain.daa / epoch_length;
        let rows: Vec<(Hash64, u64, u16, u64, u128)> = [base, qwen]
            .iter()
            .map(|c| {
                (
                    *c,
                    chain.state.epoch_counter(c).filter(|k| k.epoch_index == closed).map(|k| k.produced_blocks).unwrap_or(0),
                    chain.state.class_share_permille(c).unwrap_or(0),
                    chain.state.epoch_budgets().and_then(|b| b.budget_blocks.get(c).copied()).unwrap_or(0),
                    chain.state.class_target(c).unwrap().target,
                )
            })
            .collect();
        let realized: u64 = rows.iter().map(|r| r.1).sum();
        chain.step(Some(base), &[]);
        for (class, observed, share, budget, before) in rows {
            let after = chain.state.class_target(&class).unwrap().target;
            let per = if class == base { base_per } else { qwen_per };
            println!(
                "{:>5} {:>7} {:>5}‰ {:>9} {:>9} {:>7} {:>12} {:>14.6} {:>14.6}",
                closed,
                if class == base { "BASE-0" } else { "QWEN36" },
                share,
                (realized * share as u64 + 500) / 1000,
                observed,
                budget,
                kaspa_consensus_core::palw_pwu::palw_pwu_v1(before, per),
                odds(before),
                odds(after)
            );
        }
    }
}

/// **ADR-0054 end to end: the real `PALW-QWEN36` class earning its way off the grant floor.**
///
/// The same two-class RC assembly as `per_epoch_series_for_the_real_qwen36_class`, with the share
/// rule turned on. The entrant produces exactly the blocks its budget allows each epoch — which is
/// all a class can do — and the table follows: 1‰ was a permanent ceiling before this rule, and is
/// a starting point after it.
#[test]
fn the_real_qwen36_class_earns_share_by_producing() {
    use kaspa_consensus_core::config::params::palw_rc_params_with_qwen36;
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

    let registry: Vec<_> = (0..kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1() as u32)
        .map(|i| kaspa_consensus_core::palw_fp_devnet_v3::PalwGenesisBondSpecV1 {
            bond: PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(i)),
            pubkey: vec![7u8.wrapping_add(i as u8); 32],
            operator_pubkey: vec![21u8, i as u8, 0, 0, 0, 0, 0, 0],
            payout_payload: h(0x9A11 + i as u64),
        })
        .collect();
    let params = palw_rc_params_with_qwen36(h(0xB0), h(0x93), registry).expect("the two-class genesis assembles");
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("a ConsensusV2 network") };

    // The one difference from the measurement run: ADR-0054 is on. 250‰ per epoch, and the floor
    // keeps half the table.
    // The reserve is not a second argument: the merge converged it onto
    // `min_base_class_share_permille`, the field the audit added when 999 individually-legal grants
    // were shown to walk the floor to 1‰. Growth and granting move the same permille, so they check
    // the same bound. Set here to the half-table this run was measured against.
    let p = bundle
        .state
        .clone()
        .with_min_base_class_share_permille(500)
        .expect("half the table is a legal reserve")
        .with_class_share_growth_v1(250)
        .expect("the growth params are in range");
    let epoch_length = p.epoch_length();
    let mut ids: Vec<Hash64> = Vec::new();
    for object in &bundle.genesis_objects {
        if let Obj::ClassRegistered { class_id, .. } = object {
            ids.push(*class_id);
        }
    }
    let (base, qwen) = (ids[0], ids[1]);

    let mut chain = Chain {
        state: PalwChainStateV2::genesis(),
        params: p,
        daa: 0,
        produced: Default::default(),
        bond: kaspa_consensus_core::config::premine::premine_outpoint(0),
    };
    chain.step(None, &bundle.genesis_objects);
    println!();
    println!("=== ADR-0054: QWEN36 earning share, 250‰/epoch, floor reserve 500‰ ===");
    println!(
        "{:>5} {:>12} {:>12} {:>10} {:>12} {:>12}",
        "epoch", "QWEN share", "QWEN budget", "QWEN made", "BASE share", "QWEN target"
    );

    let mut trajectory = Vec::new();
    for _ in 0..14u64 {
        // A class can produce at most its budget; producing all of it is the only signal it has.
        let quota = chain.state.epoch_budgets().and_then(|b| b.budget_blocks.get(&qwen).copied()).unwrap_or(0);
        let mut made = 0u64;
        loop {
            let class = if made < quota {
                made += 1;
                qwen
            } else {
                base
            };
            chain.step(Some(class), &[]);
            if (chain.daa + 1).is_multiple_of(epoch_length) {
                break;
            }
        }
        let closed = chain.daa / epoch_length;
        let observed = chain.state.epoch_counter(&qwen).filter(|k| k.epoch_index == closed).map(|k| k.produced_blocks).unwrap_or(0);
        let share_before = chain.state.class_share_permille(&qwen).unwrap_or(0);
        // Crossing the boundary runs the retarget and then the share rule.
        chain.step(Some(base), &[]);
        let share_after = chain.state.class_share_permille(&qwen).unwrap_or(0);
        let base_after = chain.state.class_share_permille(&base).unwrap_or(0);
        println!(
            "{:>5} {:>11}‰ {:>12} {:>10} {:>11}‰ {:>12.6}",
            closed,
            share_after,
            quota,
            observed,
            base_after,
            odds(chain.state.class_target(&qwen).unwrap().target)
        );
        assert_eq!(share_after + base_after, 1000, "the denominator is conserved at every boundary the rule fires at");
        assert!(share_after >= share_before, "a class that filled its budget never loses share");
        trajectory.push(share_after);
    }
    assert!(trajectory.last().unwrap() > &1, "the entrant left the grant floor by producing");
    assert!(chain.state.class_share_permille(&base).unwrap() >= 500, "and the liveness floor never went below its reserve");
    println!("QWEN36 share trajectory: {trajectory:?}");
}

/// **The wiring's own three refusals** (ADR-0054 Decision 3), through `apply_palw_transition_v2`
/// rather than through the arithmetic: off is the identity, a share moves only at a boundary, and
/// an epoch nobody produced in moves nothing.
#[test]
fn the_share_rule_fires_only_where_it_should() {
    let base = h(0xBA5E);
    let qwen = h(0x9E4);

    let build = |growth: u16| {
        let mut p = params(base);
        if growth > 0 {
            p = p.with_min_base_class_share_permille(500).expect("in range").with_class_share_growth_v1(growth).expect("in range");
        }
        let floor = p.min_grantable_share_permille();
        let genesis = vec![
            Obj::BondRegistered {
                bond: bond_key(),
                pubkey: vec![7u8; 32],
                operator_pubkey: vec![21u8; 8],
                collateral: 1_000_000_000_000_000,
                payout_payload: h(0x9A4),
                capable_classes: Default::default(),
                signature: vec![9u8; 64],
            },
            registration(base, 1_000, BASE_PWU_PER_INFERENCE),
            registration(qwen, floor, QWEN_PWU_PER_INFERENCE),
        ];
        let mut chain =
            Chain { state: PalwChainStateV2::genesis(), params: p, daa: 0, produced: Default::default(), bond: bond_key().0 };
        chain.step(None, &genesis);
        chain
    };

    // **Off is the identity.** The same epoch, the same production, and not one permille moves.
    let mut off = build(0);
    let mut made = 0;
    while off.daa < EPOCH_LENGTH * 2 {
        let class = if made < 1 && off.daa < EPOCH_LENGTH {
            made += 1;
            qwen
        } else {
            base
        };
        off.step(Some(class), &[]);
    }
    assert_eq!(off.state.class_share_permille(&qwen), Some(1), "with the rule off a share is still a constant");

    // **On, and the boundary is what moves it.** Inside the epoch the table does not move; the
    // block that crosses is the one that pays.
    let mut on = build(250);
    let mut made = 0;
    while on.daa < EPOCH_LENGTH - 1 {
        let class = if made < 1 {
            made += 1;
            qwen
        } else {
            base
        };
        on.step(Some(class), &[]);
    }
    assert_eq!(on.state.class_share_permille(&qwen), Some(1), "mid-epoch, nothing has been measured yet");
    on.step(Some(base), &[]); // the crossing
    assert_eq!(on.state.class_share_permille(&qwen), Some(2), "and the boundary is where the step lands");

    // **An epoch nobody produced in decays nobody.** The floor keeps producing here, so the span is
    // not empty for it — what matters is that a silent span for the WHOLE table moves nothing.
    let mut idle = build(250);
    while idle.daa < EPOCH_LENGTH - 1 {
        idle.step(None, &[]);
    }
    let before = idle.state.class_share_permille(&qwen);
    idle.step(None, &[]);
    assert_eq!(idle.state.class_share_permille(&qwen), before, "an outage is nobody's fault and costs nobody their share");
}

/// **The shipped three-class card, under ADR-0054: both entrants earn, the floor funds both.**
///
/// `palw_rc_shipped_params` is what a node actually runs — the floor, the Qwen3.6 hybrid tier and
/// the Qwen2.5-A16 dense tier, each pinned by its own artifact root. The share rule and the
/// three-class card landed on separate branches; this is the property that only exists once they
/// are together: two classes drawing on ONE reservoir, in one epoch, without either of them or the
/// pair of them breaching the floor's reserve.
#[test]
fn the_shipped_three_class_card_grows_both_entrants() {
    use kaspa_consensus_core::config::params::palw_rc_shipped_params;
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

    let params = palw_rc_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        panic!("the shipped RC card assembles a ConsensusV2 network")
    };
    let classes: Vec<Hash64> = bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            Obj::ClassRegistered { class_id, .. } => Some(*class_id),
            _ => None,
        })
        .collect();
    assert_eq!(classes.len(), 3, "the floor, the hybrid tier and the dense tier");
    let (base, entrants) = (classes[0], &classes[1..]);
    assert_eq!(base, bundle.state.base_class_id(), "the floor registers first");

    let p = bundle.state.clone();
    assert!(p.class_growth_permille() > 0, "the shipped bundle carries ADR-0054");
    let epoch_length = p.epoch_length();
    let reserve = p.base_class_reserve_permille();
    let mut chain = Chain {
        state: PalwChainStateV2::genesis(),
        params: p,
        daa: 0,
        produced: Default::default(),
        bond: kaspa_consensus_core::config::premine::premine_outpoint(0),
    };
    chain.step(None, &bundle.genesis_objects);
    // **The table the card actually mints.** The constants name what each tier KEEPS, and the
    // hybrid declares more than that because the dense tier's donation dilutes it — so the only
    // honest check is to fold the real transition over the real objects and read the result.
    let floor_share = chain.state.class_share_permille(&base).unwrap();
    let entrant_shares: Vec<u16> = entrants.iter().map(|c| chain.state.class_share_permille(c).unwrap()).collect();
    assert_eq!(
        entrant_shares,
        vec![
            kaspa_consensus_core::config::params::PALW_RC_GENESIS_QWEN36_SHARE_PERMILLE,
            kaspa_consensus_core::config::params::PALW_RC_GENESIS_QWEN25_A16_SHARE_PERMILLE
        ],
        "each tier keeps what its constant names, dilution inverted"
    );
    assert_eq!(floor_share as u32 + entrant_shares.iter().map(|s| u32::from(*s)).sum::<u32>(), 1000);
    assert!(floor_share >= reserve, "and the floor starts at or above its own reserve");
    println!("minted table: floor {floor_share}‰, tiers {entrant_shares:?}");

    println!();
    println!("=== the shipped three-class card, ADR-0054 on ===");
    println!("{:>5} {:>12} {:>12} {:>12}", "epoch", "floor ‰", "entrant A ‰", "entrant B ‰");
    for _ in 0..10u64 {
        // Each entrant produces every block its own budget allows; the floor takes the rest.
        let mut quota: Vec<u64> =
            entrants.iter().map(|c| chain.state.epoch_budgets().and_then(|b| b.budget_blocks.get(c).copied()).unwrap_or(0)).collect();
        loop {
            let next = entrants.iter().zip(quota.iter_mut()).find(|(_, left)| **left > 0);
            let class = match next {
                Some((class, left)) => {
                    *left -= 1;
                    *class
                }
                None => base,
            };
            chain.step(Some(class), &[]);
            if (chain.daa + 1).is_multiple_of(epoch_length) {
                break;
            }
        }
        chain.step(Some(base), &[]); // the crossing pays
        let floor_share = chain.state.class_share_permille(&base).unwrap();
        let shares: Vec<u16> = entrants.iter().map(|c| chain.state.class_share_permille(c).unwrap()).collect();
        println!("{:>5} {:>11}‰ {:>11}‰ {:>11}‰", chain.daa / epoch_length - 1, floor_share, shares[0], shares[1]);
        assert_eq!(floor_share as u32 + shares.iter().map(|s| u32::from(*s)).sum::<u32>(), 1000, "three classes, one denominator");
        assert!(floor_share >= reserve, "the floor funds both entrants and still keeps its reserve");
    }
    for (entrant, share) in entrants.iter().zip(entrants.iter().map(|c| chain.state.class_share_permille(c).unwrap())) {
        assert!(share > 1, "class {entrant} earned its way off the grant floor");
    }
}

/// Mirrors the block-level run at the transition, to tell a rule from its plumbing: the two-class
/// card, the hybrid filling exactly its funded budget in epoch 0, and the crossing block.
#[test]
fn a_funded_tier_that_fills_its_budget_grows_at_the_first_boundary() {
    use kaspa_consensus_core::config::params::palw_rc_params_with_qwen36;
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

    let registry: Vec<_> = (0..kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1() as u32)
        .map(|i| kaspa_consensus_core::palw_fp_devnet_v3::PalwGenesisBondSpecV1 {
            bond: PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(i)),
            pubkey: vec![7u8.wrapping_add(i as u8); 32],
            operator_pubkey: vec![21u8, i as u8, 0, 0, 0, 0, 0, 0],
            payout_payload: h(0x9A11 + i as u64),
        })
        .collect();
    let params = palw_rc_params_with_qwen36(h(0xB0), h(0x93), registry).expect("assembles");
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!() };
    let p = bundle.state.clone();
    let epoch_length = p.epoch_length();
    let ids: Vec<Hash64> = bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            Obj::ClassRegistered { class_id, .. } => Some(*class_id),
            _ => None,
        })
        .collect();
    let (base, qwen) = (ids[0], ids[1]);
    let mut chain = Chain {
        state: PalwChainStateV2::genesis(),
        params: p,
        daa: 0,
        produced: Default::default(),
        bond: kaspa_consensus_core::config::premine::premine_outpoint(0),
    };
    chain.step(None, &bundle.genesis_objects);
    let quota = chain.state.epoch_budgets().and_then(|b| b.budget_blocks.get(&qwen).copied()).unwrap_or(0);
    let opening = chain.state.class_share_permille(&qwen).unwrap();
    println!("opening share {opening}‰, epoch-0 budget {quota}");
    let mut made = 0u64;
    while chain.daa < epoch_length - 1 {
        let class = if made < quota {
            made += 1;
            qwen
        } else {
            base
        };
        chain.step(Some(class), &[]);
    }
    let counted = chain.state.epoch_counter(&qwen).map(|c| (c.epoch_index, c.produced_blocks));
    let budget_now = chain.state.epoch_budgets().map(|b| (b.epoch_index, b.budget_blocks.get(&qwen).copied()));
    println!("before the crossing: daa {}, counter {:?}, budget table {:?}", chain.daa, counted, budget_now);
    chain.step(Some(base), &[]); // the crossing
    println!(
        "after the crossing: qwen {}‰, base {}‰",
        chain.state.class_share_permille(&qwen).unwrap(),
        chain.state.class_share_permille(&base).unwrap()
    );
    assert!(chain.state.class_share_permille(&qwen).unwrap() > opening, "a tier that filled its budget grows");
}

/// What the genesis bond outpoints actually hold — the ceiling on how heavy a class the shipped
/// registry can carry, since `verify_palw_genesis_v2` refuses a bond declaring more than its
/// outpoint holds.
#[test]
fn what_the_genesis_bond_outpoints_hold() {
    let net = kaspa_consensus_core::network::NetworkId::with_suffix(kaspa_consensus_core::network::NetworkType::Testnet, 11);
    let utxos = kaspa_consensus_core::config::premine::genesis_premine_utxos_for(net);
    let bonds = kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1();
    let mut total = 0u64;
    for i in 0..bonds as u32 {
        let outpoint = kaspa_consensus_core::config::premine::premine_outpoint(i);
        let amount = utxos.get(&outpoint).map(|e| e.amount).unwrap_or(0);
        total += amount;
        println!("bond {i}: {amount} sompi = {:.2} MSK", amount as f64 / 1e8);
    }
    println!("total across {bonds} bonds: {:.2} MSK", total as f64 / 1e8);
    println!(
        "floor-sized collateral {:.2} MSK, Qwen3.6-sized {:.2} MSK",
        kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_collateral_for_claim_lifetime_v1(7_708) as f64 / 1e8,
        kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_collateral_for_claim_lifetime_v1(2_685_440) as f64 / 1e8
    );
}

/// **The bind-window gate answers about the class that costs most to hold.**
///
/// It read the BASE class's per-claim exposure, which on a network that registers a model tier
/// beside the floor is the cheapest claim a bond will ever carry — so a registry funded for the
/// floor passed while the genesis was allocating cadence to a tier whose claims reserve 348× as
/// much. Measured through the block path: fifteen blocks accepted against a two-hundred-block
/// allowance, then `the bond's exposure ceiling leaves no room for another claim`, which is the
/// deadlock this gate's own comment describes.
#[test]
fn the_bind_window_gate_measures_the_dearest_class() {
    use kaspa_consensus_core::config::params::palw_rc_params_with_classes;
    use kaspa_consensus_core::palw_genesis_v2::{PalwGenesisV2Error, verify_palw_genesis_v2};
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

    let registry: Vec<_> = (0..kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1() as u32)
        .map(|i| kaspa_consensus_core::palw_fp_devnet_v3::PalwGenesisBondSpecV1 {
            bond: PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(i)),
            pubkey: vec![7u8.wrapping_add(i as u8); 32],
            operator_pubkey: vec![21u8, i as u8, 0, 0, 0, 0, 0, 0],
            payout_payload: h(0x9A11 + i as u64),
        })
        .collect();
    let params = palw_rc_params_with_classes(h(0xB0), h(0x93), Some(h(0x25)), registry).expect("the card assembles");
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("a ConsensusV2 network") };

    // The card funds its bonds for the dearest class, so the floor's own sizing is far below what
    // the registry declares — that gap IS the fix.
    let floor_sized = kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_collateral_for_claim_lifetime_v1(7_708);
    let declared = bundle
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            Obj::BondRegistered { collateral, .. } => Some(*collateral),
            _ => None,
        })
        .expect("the card registers bonds");
    assert!(declared > floor_sized * 100, "the registry is sized for the tiers, not for the floor: {declared} vs {floor_sized}");

    // And the gate refuses the registry the old sizing would have shipped.
    // The catalog preimage, rebuilt from the same public registrations the card assembles from —
    // the bundle carries only its root.
    let mut entries = Vec::new();
    let (_, base_catalog) = kaspa_consensus_core::palw_base0_profile::palw_rc_base0_registration_v1(h(0xB0)).expect("derives");
    entries.push(base_catalog.entries().first().expect("the floor").clone());
    for object in &bundle.genesis_objects {
        let Obj::ClassRegistered { class_id, artifact_root, share_permille, slash_value_per_pwu, initial_target, .. } = object else {
            continue;
        };
        if *class_id == bundle.state.base_class_id() {
            continue;
        }
        // **Through `qwen36_registration_v3`, which is what the chain registers.** This asked
        // `qwen36_registration_v1` first, and v1 declares the graph
        // `Qwen36Backend::from_registered_profile` refuses — so its class id never matched, the
        // `filter` dropped it, and the qwen25 fallback built a SECOND qwen25 entry for the qwen36
        // registration. Two entries with one class id, and the catalog constructor said so:
        // "must be ascending and unique by class id". The tier builder a test reaches for has to
        // be the one the genesis reached for, or the fallback quietly answers a different question.
        let built = kaspa_consensus_core::palw_qwen36_profile::qwen36_registration_v3(
            *artifact_root,
            *share_permille,
            *slash_value_per_pwu,
            *initial_target,
        )
        .ok()
        .filter(|(_, entry, _)| entry.class_id == *class_id)
        .or_else(|| {
            // …and `_v2` for the dense tier, for the same reason: this rebuilds the shipped
            // catalog, so every entry has to come from the builder the shipped assembly used or
            // the reconstructed root is a different ruleset's.
            kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_registration_v2(
                *artifact_root,
                *share_permille,
                *slash_value_per_pwu,
                *initial_target,
            )
            .ok()
        })
        .expect("one of the two tiers derives this registration");
        entries.push(built.1);
    }
    entries.sort_by(|a, b| a.class_id.cmp(&b.class_id));
    let catalog = kaspa_consensus_core::palw_mode_v2::PalwClassCatalogV2::new(entries).expect("well-formed");
    let starved: Vec<_> = bundle
        .genesis_objects
        .iter()
        .cloned()
        .map(|o| match o {
            Obj::BondRegistered { bond, pubkey, operator_pubkey, payout_payload, capable_classes, signature, .. } => Obj::BondRegistered {
                bond,
                pubkey,
                operator_pubkey,
                collateral: floor_sized,
                payout_payload,
                capable_classes,
                signature,
            },
            other => other,
        })
        .collect();
    let utxos = kaspa_consensus_core::config::premine::genesis_premine_utxos_for(params.net);
    let err = verify_palw_genesis_v2(bundle, &catalog, &starved, |outpoint| utxos.get(outpoint).map(|e| e.amount))
        .expect_err("a registry that cannot carry the tiers it funds must be refused");
    assert!(
        matches!(err, PalwGenesisV2Error::BondCannotSustainBindWindow { .. }),
        "and refused for the reason that is true, got {err:?}"
    );
}

/// **Step-5 witness for the root re-pin: the class id is a function of the graph, and the graph
/// this build SERVES is the one the chain registers.** Printed from every derivation that exists,
/// so a divergence between them (the C5 pattern) cannot hide behind any one of them.
///
/// This used to compare the registration against `qwen36_profile_v1`, and it was asserting the
/// wrong equality rather than catching a defect. There are two real ids and the difference between
/// them is the point: `qwen36_class_id_v1` names the graph
/// `Qwen36Backend::from_registered_profile` REFUSES ("op SoftMax with operand
/// `blk.{layer}.ffn_router_topk.a16` is not arithmetic this build serves"), and `qwen36_class_id_v3`
/// names the `graph-v3` row this build's SDK dispatches on. A chain that registered the first would
/// have a tier with no backend, no capture and no court — so the assertion has to be that the
/// registration is the SECOND, and that the two are still distinct.
#[test]
fn the_qwen36_class_id_is_the_same_through_every_derivation() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    use kaspa_consensus_core::palw_qwen36_profile::{qwen36_class_id_v1, qwen36_class_id_v3};
    let servable = qwen36_class_id_v3();
    let unservable = qwen36_class_id_v1();
    println!("qwen36 graph-v3 (this build serves it): {servable}");
    println!("qwen36 v1 declaration (this build refuses it): {unservable}");
    assert_ne!(servable, unservable, "the two declarations are different classes — that is why the re-pin happened");
    let params = kaspa_consensus_core::config::params::palw_rc_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!() };
    let mut saw_the_tier = false;
    for object in &bundle.genesis_objects {
        if let Obj::ClassRegistered { class_id, artifact_root, share_permille, .. } = object {
            println!("registered: {class_id} root {artifact_root} share {share_permille}");
            if *artifact_root == kaspa_consensus_core::config::params::PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT {
                saw_the_tier = true;
                assert_eq!(*class_id, servable, "the chain must register the graph this build can serve");
                assert_ne!(*class_id, unservable, "and never the one it refuses");
            }
        }
    }
    assert!(saw_the_tier, "the shipped RC registers the qwen36 tier — if it stopped, this test is measuring nothing");
}

/// **The whole loop, closed: per-model difficulty and cross-model share co-adjust to a stable
/// equilibrium** — the ADR-0054 share walk and the per-class retarget running TOGETHER, with
/// production driven by each model's own compute capacity and its CURRENT target, not scripted.
///
/// Two entrants join a floor-carried chain at the minimum grantable share with a 10× compute
/// asymmetry: FAST can run 40 inferences per epoch, HEAVY 4. Each epoch a model wins
/// `min(budget, ⌊capacity × odds(target)⌋)` blocks — the deterministic expectation of the
/// lottery a real producer grinds — the floor fills the rest of the cadence, and every boundary
/// then walks shares (a filled budget takes `share × growth‰` from the floor, ADR-0054) and
/// retargets difficulty (share of realized production, audit-H1 renormalized over producers).
///
/// The equilibrium law this pins, which no single test held before:
///   1. **share converges to what the model's compute can actually fill.** Both entrants walk
///      up from the grant floor; each stalls where its capacity stops filling the next budget —
///      so the 10× compute asymmetry ends as ~10× share asymmetry, decided by realized
///      production alone (no declared cost anywhere in the signal path);
///   2. **difficulty absorbs the asymmetry on the way**: the heavy model's target EASES (its
///      odds rise) so its few inferences keep winning its share-implied frequency — "share
///      frequency appropriate per model" is the retarget seeing compute cost through the only
///      unforgeable signal, the blocks themselves;
///   3. **the floor's reserve is never walked away** (Σ shares = 1000‰ at every boundary, floor
///      ≥ the reserve, liveness intact);
///   4. **down-regulation is as automatic as up**: a model that goes silent decays back toward
///      the grant floor, and its target holds still — a span it sat out says nothing about its
///      difficulty in either direction.
#[test]
fn difficulty_and_share_co_adjust_across_models_to_equilibrium() {
    let base = h(0xBA5E);
    let fast = h(0xFA57);
    let heavy = h(0x4EA1);
    let p = params(base).with_class_share_growth_v1(250).expect("the ADR-0054 walk, at the shipped 250‰ step");
    let grant_floor = p.min_grantable_share_permille();
    let reserve = p.base_class_reserve_permille();
    let mut chain = Chain { state: PalwChainStateV2::genesis(), params: p, daa: 0, produced: Default::default(), bond: bond_key().0 };

    chain.step(
        None,
        &[
            Obj::BondRegistered {
                bond: bond_key(),
                pubkey: vec![7u8; 32],
                operator_pubkey: vec![21u8; 8],
                collateral: 1_000_000_000_000_000,
                payout_payload: h(0x9A4),
                capable_classes: Default::default(),
                signature: vec![9u8; 64],
            },
            registration(base, 1_000, BASE_PWU_PER_INFERENCE),
            registration(fast, grant_floor, QWEN_PWU_PER_INFERENCE),
            registration(heavy, grant_floor, QWEN_PWU_PER_INFERENCE),
        ],
    );

    const FAST_CAPACITY: u64 = 40; // inferences per epoch this model's hardware affords
    const HEAVY_CAPACITY: u64 = 4; // 10× the compute per inference, so a tenth the attempts
    const EPOCHS: u64 = 22;
    const SILENCE_FROM: u64 = 18; // the heavy model stops attempting here (down-regulation arm)

    let mut heavy_share_at_silence = 0u16;
    let mut heavy_target_at_silence = 0u128;
    let mut heavy_odds_early = 0.0f64;

    println!();
    println!("=== co-adjustment to equilibrium (fast 40/epoch, heavy 4/epoch, growth 250‰) ===");
    println!("{:>5} {:>6} {:>6} {:>7} {:>9} {:>10}", "epoch", "class", "share", "budget", "produced", "odds");

    for epoch in 0..EPOCHS {
        // What each model can WIN this epoch under its current difficulty: its attempt capacity
        // times its odds, held to its budget exactly the way a real producer holds.
        let plan: std::collections::BTreeMap<Hash64, u64> = [(fast, FAST_CAPACITY), (heavy, HEAVY_CAPACITY)]
            .into_iter()
            .map(|(class, capacity)| {
                let capacity = if class == heavy && epoch >= SILENCE_FROM { 0 } else { capacity };
                let target = chain.state.class_target(&class).expect("registered").target;
                let budget = chain.state.epoch_budgets().and_then(|b| b.budget_blocks.get(&class).copied()).unwrap_or(0);
                let expected_wins = ((capacity as f64) * odds(target)).floor() as u64;
                (class, expected_wins.min(budget))
            })
            .collect();
        let mut made: std::collections::BTreeMap<Hash64, u64> = Default::default();
        loop {
            // Entrant blocks are spread through the epoch; the floor carries every other slot.
            let class = [fast, heavy]
                .into_iter()
                .find(|c| {
                    let want = plan.get(c).copied().unwrap_or(0);
                    let did = made.get(c).copied().unwrap_or(0);
                    did < want && chain.daa.is_multiple_of(EPOCH_LENGTH / want.clamp(1, EPOCH_LENGTH))
                })
                .unwrap_or(base);
            *made.entry(class).or_default() += 1;
            chain.step(Some(class), &[]);
            if (chain.daa + 1).is_multiple_of(EPOCH_LENGTH) {
                break;
            }
        }
        let closed = chain.daa / EPOCH_LENGTH;
        let produced: std::collections::BTreeMap<Hash64, u64> = [base, fast, heavy]
            .iter()
            .map(|c| (*c, chain.state.epoch_counter(c).filter(|k| k.epoch_index == closed).map(|k| k.produced_blocks).unwrap_or(0)))
            .collect();
        // The boundary block: closes the span, walks the shares, retargets both lanes.
        chain.step(Some(base), &[]);

        let mut total_share = 0u32;
        for class in [base, fast, heavy] {
            let share = chain.state.class_share_permille(&class).unwrap_or(0);
            total_share += share as u32;
            let target = chain.state.class_target(&class).expect("registered").target;
            let budget = chain.state.epoch_budgets().and_then(|b| b.budget_blocks.get(&class).copied()).unwrap_or(0);
            println!(
                "{:>5} {:>6} {:>5}‰ {:>7} {:>9} {:>10.6}",
                closed,
                if class == base {
                    "BASE"
                } else if class == fast {
                    "FAST"
                } else {
                    "HEAVY"
                },
                share,
                budget,
                produced.get(&class).copied().unwrap_or(0),
                odds(target)
            );
        }
        // (3) The table is conserved and the liveness floor keeps its reserve, at EVERY boundary.
        assert_eq!(total_share, 1_000, "the share table must stay a partition of the whole");
        assert!(
            chain.state.class_share_permille(&base).unwrap_or(0) >= reserve,
            "the walk must never draw the floor below its reserve"
        );

        if epoch == 2 {
            heavy_odds_early = odds(chain.state.class_target(&heavy).unwrap().target);
        }
        if closed == SILENCE_FROM {
            heavy_share_at_silence = chain.state.class_share_permille(&heavy).unwrap_or(0);
            heavy_target_at_silence = chain.state.class_target(&heavy).unwrap().target;
        }
    }

    let fast_share = chain.state.class_share_permille(&fast).unwrap_or(0);
    let heavy_share = chain.state.class_share_permille(&heavy).unwrap_or(0);
    let heavy_odds_final = odds(chain.state.class_target(&heavy).unwrap().target);

    // (1) Share followed capacity: the fast model walked far past the heavy one, and each sits
    // in the band its own compute affords (fast stalls where 40 wins stop filling the next
    // budget; heavy where 4 do). The 10× compute asymmetry became share asymmetry on its own.
    assert!((25..=60).contains(&fast_share), "the fast model should stall near its 40-win capacity, got {fast_share}‰");
    assert!(
        fast_share >= 5 * heavy_share.max(1),
        "a 10× capacity gap must open a wide share gap: fast {fast_share}‰ vs heavy {heavy_share}‰"
    );

    // (2) Difficulty absorbed the asymmetry while the heavy model produced: its odds rose from
    // the boot value toward certainty so its few inferences kept their share-implied frequency.
    assert!(
        heavy_odds_final > heavy_odds_early,
        "the heavy model's target must EASE while it fills at a tenth the attempt rate ({heavy_odds_early} -> {heavy_odds_final})"
    );

    // (4) Silence decayed the share back toward the grant floor and did not move the target.
    assert!(heavy_share < heavy_share_at_silence, "a silent model's share must decay ({heavy_share_at_silence}‰ -> {heavy_share}‰)");
    assert!(heavy_share >= grant_floor, "decay stops at the grant floor, never below");
    assert_eq!(
        chain.state.class_target(&heavy).unwrap().target,
        heavy_target_at_silence,
        "a span the model sat out says nothing about its difficulty — the target must hold still"
    );
}
