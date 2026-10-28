//! **R12-AUTHORITY (lane R12, 2026-10-11): the private-Final race under the only schedule a release may arm** —
//! `palw_fork_choice_commitment_v1 <= palw_fork_choice_rule_e_v1 <= palw_dns_retirement_v1` (ADR-0178 §6, the user's ordering rule).
//!
//! `private_final.rs` measured the race with the retirement armed ALONE — a schedule `validate_palw_v2` refuses
//! (`PALW_DNS_RETIREMENT_NEEDS_RULE_E_V1`). This module runs the same race on both schedules over the same script:
//!
//! * `illegal` — release + capacity fences and the retirement at DAA 1, rule E and the commitment unarmed (the refused schedule);
//! * `legal` — the same plus the commitment and rule E at DAA 1 (the order holds; the only refusals left are the two "not armable in
//!   this binary" refusals, asserted by name, so every other check of `validate_palw_v2` passes).
//!
//! First expectation, and how the run corrected it: "with the honest side carrying MORE bonds' attempts, the illegal schedule lets
//! the private Final's greater frontier reverse X". It did not — the honest attempts also made the public branch GHOSTDAG-heavier,
//! and the sink search never offered the lighter private branch to any comparator (V1). That run is kept as its own case; the
//! discriminating case gives the private side MORE blue work from FEWER bonds. With the private side carrying MORE bonds, both
//! schedules reverse X: under rule E the
//! security of a pre-safe effect is a bonded-participation majority assumption, not a proof (the user's ECON ruling 10-10:
//! a malicious bond majority is the BFT-equivalent residual).
//!
//! Seams, named: PoW skipped; the harness holds every genesis seat key (the private quorum signs with them); synthetic floor
//! traces; EVM inert. No Final / frontier / participation fact is planted — claims reach Final through the node's own lifecycle.
//! This is a pre-safe race (native `safe` needs >= 5,400 DAA), not a certified-effect attack and not a common-prefix proof.
use super::private_final::own_final;
use super::*;
use kaspa_consensus_core::config::params::{PALW_T12_POST_LAUNCH_FENCES_V1, palw_t12_arm_capacity_fences_v1};
use kaspa_consensus_core::palw_fork_choice_rule_e_v1::{
    PALW_DNS_RETIREMENT_NEEDS_RULE_E_V1, PALW_FORK_CHOICE_RULE_E_UNARMABLE_V1, PALW_RULE_E_ABOVE_DNS_RETIREMENT_V1,
    PALW_RULE_E_NEEDS_FORK_CHOICE_COMMITMENT_V1, PALW_RULE_E_PARTICIPATION_DEPTH_DAA_V1 as W_P,
};
use kaspa_consensus_core::palw_mode_v2::PalwModeV2Error;
use kaspa_consensus_core::palw_native_settlement_v1::{PalwDnsRetirementV1, PalwSettlementPolicyV1};
use kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2;

/// TEST policy values only (the user decides D / W / caps / horizon; see the policy proposal §10–§11).
fn test_retirement() -> PalwDnsRetirementV1 {
    PalwDnsRetirementV1 {
        activation: ForkActivation::new(1),
        settlement: PalwSettlementPolicyV1 {
            settled_anchor_depth: 1,
            unique_mature_work: 1,
            max_operator_permille: 1000,
            max_class_permille: 1000,
        },
        legacy_evidence_horizon_daa: 5,
    }
}

fn invalid(p: &kaspa_consensus_core::config::params::Params) -> Option<&'static str> {
    match p.validate_palw_v2() {
        Ok(()) => None,
        Err(PalwModeV2Error::Invalid(why)) => Some(why),
        Err(other) => panic!("an unexpected refusal: {other:?}"),
    }
}

/// The release and capacity fence sets at DAA 1, the retirement at DAA 1, and — when `legal` — the commitment and rule E at DAA 1.
/// Asserts which refusals the schedule meets, by name, so the node runs a ruleset every other check of `validate_palw_v2` accepts.
fn duel_rfc12(legal: bool) -> Duel {
    let (config, _, premine, floats) = t12_with_harness_cards();
    let mut params = config.params.clone();
    for fence in PALW_T12_POST_LAUNCH_FENCES_V1 {
        (fence.set)(&mut params, Some(ForkActivation::new(1)));
    }
    palw_t12_arm_capacity_fences_v1(&mut params, Some(ForkActivation::new(1))).expect("the complete capacity fence set");
    let mut config = ConfigBuilder::new(params).skip_proof_of_work().build();
    config.params.validate_palw_v2().expect("the complete release + capacity set validates");
    config.params.palw_dns_retirement = Some(test_retirement());
    // The retirement alone: refused by the ordering rule, which `validate_palw_v2` asks LAST — so nothing else refuses it.
    assert_eq!(invalid(&config.params), Some(PALW_DNS_RETIREMENT_NEEDS_RULE_E_V1), "the retirement alone meets only the order");
    if legal {
        // Rule E above the retirement is refused by name.
        config.params.palw_fork_choice_rule_e_v1 = Some(ForkActivation::new(2));
        assert_eq!(
            config.params.validate_palw_fork_choice_rule_e_order_v1(),
            Err(PalwModeV2Error::Invalid(PALW_RULE_E_ABOVE_DNS_RETIREMENT_V1))
        );
        // Rule E at the retirement without the commitment: the second half of the order, still asked last.
        config.params.palw_fork_choice_rule_e_v1 = Some(ForkActivation::new(1));
        assert_eq!(invalid(&config.params), Some(PALW_RULE_E_NEEDS_FORK_CHOICE_COMMITMENT_V1), "rule E without the commitment");
        // The full order: satisfied. What remains are the two "no release height" refusals.
        config.params.palw_fork_choice_commitment_v1 = Some(ForkActivation::new(1));
        assert_eq!(config.params.validate_palw_fork_choice_rule_e_order_v1(), Ok(()), "commitment <= rule E <= retirement");
        assert!(
            invalid(&config.params).is_some_and(|why| why.starts_with("palw_fork_choice_commitment_v1 cannot be armed")),
            "the commitment's own refusal: {:?}",
            invalid(&config.params)
        );
        assert_eq!(
            config.params.validate_palw_fork_choice_rule_e_v1(),
            Err(PalwModeV2Error::Invalid(PALW_FORK_CHOICE_RULE_E_UNARMABLE_V1))
        );
    }
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else {
        unreachable!("ConsensusV2")
    };
    let bundle = bundle.clone();
    let victim = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let attacker = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let pay = |to: ScriptPublicKey| {
        let (outpoint, entry) = floats[1].clone();
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(entry.amount - 300_000, to)],
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        sign_spend(&mut tx, entry, 1, config.params.storage_mass_parameter);
        tx
    };
    let x = pay(card_payout_spk(5));
    let y = pay(card_payout_spk(6));
    Duel { config, victim, attacker, x, y, nonce: 1 << 40, floats }
}

/// What one race showed.
#[derive(Debug)]
struct Race {
    flipped: bool,
    x_y: (bool, bool),
    public_participation_cards: Vec<usize>,
    private_participation_cards: Vec<usize>,
    depth_daa: u64,
}

/// The private-Final race: a shared prefix, the public Final carrying X (card 0, anchored by card 7) plus `public_extra` cards'
/// attempts, the private Final carrying Y (card 2, anchored by card 7) plus `private_extra` cards' attempts after four
/// two-sibling slots; the victim's chain runs to the attacker's DAA, then the private branch is released to it block by block.
async fn race(tag: &str, legal: bool, public_extra: &[usize], private_extra: &[usize]) -> Race {
    let mut d = duel_rfc12(legal);
    for _ in 0..3 {
        honest_slot_mirrored(&mut d).await;
    }
    let fork = d.victim.sink();
    assert!(d.victim.vp().dns_retired_at(d.victim.daa_of(fork)), "{tag}: the race runs past the retirement");
    assert_eq!(d.victim.vp().palw_rule_e_active_at(d.victim.daa_of(fork)), legal, "{tag}: rule E armed exactly on the legal schedule");
    let public_claim = own_final(&mut d.victim, 0, d.x.clone(), d.floats[0].clone()).await;
    for &card in public_extra {
        d.victim.attempt(card, 1_000, Vec::new(), &|_| true).await;
    }
    for _ in 0..4 {
        private_slot(&mut d.attacker, &mut d.nonce, 2, vec![]).await;
    }
    let private_claim = own_final(&mut d.attacker, 2, d.y.clone(), d.floats[0].clone()).await;
    for &card in private_extra {
        d.attacker.attempt(card, 1_000, Vec::new(), &|_| true).await;
    }
    while d.victim.daa_of(d.victim.sink()) < d.attacker.daa_of(d.attacker.sink()) {
        d.victim.heartbeat(d.config.params.target_time_per_block(), vec![]).await;
    }
    let depth_daa = d.victim.daa_of(d.victim.sink()).saturating_sub(d.victim.daa_of(fork));
    assert!(depth_daa > W_P, "{tag}: both tips stand past W_p above the fork ({depth_daa} DAA)");
    assert!(bs(&d.victim, d.victim.sink()) - bs(&d.victim, fork) < d.config.params.finality_depth(), "{tag}: inside finality");
    let public = d.victim.vp().palw_candidate_order_v2(d.victim.sink()).unwrap();
    let private = d.attacker.vp().palw_candidate_order_v2(d.attacker.sink()).unwrap();
    assert!(private.safe_frontier_blue_score > public.safe_frontier_blue_score, "{tag}: the private Final advances its own frontier");
    assert!(matches!(d.victim.tip_state().1.claim(&public_claim).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
    assert!(matches!(d.attacker.tip_state().1.claim(&private_claim).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
    assert_eq!(payments(&format!("{tag} before release"), &d), (true, false));
    let blocks = blocks_in_topological_order(&d.attacker);
    let at = blocks.iter().position(|b| b.header.hash == fork).unwrap();
    let released = release(tag, &mut d.victim, &blocks[at + 1..], fork, &mut d.nonce, 0).await;
    assert!(released.refused.is_empty(), "{tag}: the victim validates every private block");
    let x_y = payments(&format!("{tag} after release"), &d);
    let distinct = |base: [usize; 2], extra: &[usize]| {
        let mut cards: Vec<usize> = base.into_iter().chain(extra.iter().copied()).collect();
        cards.sort();
        cards.dedup();
        cards
    };
    let (public_cards, private_cards) = (distinct([0, 7], public_extra), distinct([2, 7], private_extra));
    let r = Race {
        flipped: released.flipped(),
        x_y,
        public_participation_cards: public_cards,
        private_participation_cards: private_cards,
        depth_daa,
    };
    eprintln!("[R12-AUTHORITY {tag}] {r:?}; keys public {public:?} private {private:?}");
    r
}

/// **Measured first (2026-10-11), kept as its own case.** The honest side carries more bonds' attempts AND more blue work (five
/// attempts against two): the private branch is never offered to the comparator (V1, ADR-0178 §1 — the sink search weighs only
/// GHOSTDAG-heavier tips), so X stands under both schedules. The refused schedule holds here by blue work, not by any PALW key.
#[tokio::test]
async fn rfc12_authority_a_heavier_honest_bond_majority_keeps_x_under_both_orders() {
    kaspa_core::log::try_init_logger("warn");
    let illegal = race("heavier honest majority, retirement alone (refused schedule)", false, &[3, 4, 5], &[]).await;
    assert!(!illegal.flipped && illegal.x_y == (true, false), "{illegal:?}");
    let legal = race("heavier honest majority, commitment <= rule E <= retirement", true, &[3, 4, 5], &[]).await;
    assert!(!legal.flipped && legal.x_y == (true, false), "{legal:?}");
}

/// **The discriminating case.** The honest side carries more BONDS (cards 0, 7, 3, 4, 5) while the private side carries more blue
/// work from FEWER bonds (card 2 attempting repeatedly beside 7). Expected before running: the refused schedule takes the heavier
/// private branch with its greater frontier and reverses X; the legal schedule ranks participation first past `W_p` and keeps X.
#[tokio::test]
async fn rfc12_authority_an_honest_bond_majority_on_the_lighter_branch_keeps_x_only_under_the_legal_order() {
    kaspa_core::log::try_init_logger("warn");
    let one_bond = [2usize; 6];
    let illegal = race("lighter honest majority, retirement alone (refused schedule)", false, &[3, 4, 5], &one_bond).await;
    assert!(
        illegal.flipped && illegal.x_y == (false, true),
        "the refused schedule: the heavier private branch reverses X: {illegal:?}"
    );
    let legal = race("lighter honest majority, commitment <= rule E <= retirement", true, &[3, 4, 5], &one_bond).await;
    assert!(!legal.flipped && legal.x_y == (true, false), "the legal schedule: participation first, X stands: {legal:?}");
}

/// The private side carries more bonds' attempts (cards 2, 7, 3, 4, 5 against 0, 7): the residual — a bond-majority private branch
/// reverses a pre-safe effect under either schedule. Rule E converts the pre-safe guarantee into a bonded-participation majority
/// assumption; it does not remove it.
#[tokio::test]
async fn rfc12_authority_a_private_bond_majority_reverses_a_pre_safe_effect_under_both_orders() {
    kaspa_core::log::try_init_logger("warn");
    let illegal = race("private majority, retirement alone (refused schedule)", false, &[], &[3, 4, 5]).await;
    assert!(illegal.flipped && illegal.x_y == (false, true), "{illegal:?}");
    let legal = race("private majority, commitment <= rule E <= retirement", true, &[], &[3, 4, 5]).await;
    assert!(legal.flipped && legal.x_y == (false, true), "{legal:?}");
}
