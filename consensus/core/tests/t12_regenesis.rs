//! **testnet-12's regenesis, pinned.** The facts an operator would otherwise have to re-derive by
//! reading four files, asserted so a change to any of them fails here rather than on a live chain.
use kaspa_consensus_core::config::params::*;
use kaspa_consensus_core::config::premine::*;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;

fn t12() -> NetworkId {
    NetworkId::with_suffix(NetworkType::Testnet, 12)
}

/// The community table is 16 entries summing to 758M MSK, every address distinct, and it CONTAINS
/// testnet-11's thirteen — a regenesis carries the allocations across, it does not restart them.
#[test]
fn t12_community_table_is_t11_plus_three() {
    let total: u64 = TESTNET12_COMMUNITY_ALLOCATIONS.iter().map(|(_, msk)| *msk).sum();
    assert_eq!(TESTNET12_COMMUNITY_ALLOCATIONS.len(), 16, "13 carried over + maruko + nyanmi-1828 + tetsu31's LLM address");
    assert_eq!(total, 758_000_000, "758M MSK");
    assert_eq!(TESTNET12_COMMUNITY_SOMPI, total * kaspa_consensus_core::constants::SOMPI_PER_KASPA);
    // Every t11 entry is still allocated, at the same amount, EXCEPT the one address the operator
    // replaced — tetsu31's 2026-08-28 one, superseded by their 2026-09-06 address.
    const TETSU31_SUPERSEDED: &str =
        "misakatest:qfvt2l0a92ang7m370srfkfq7v7mpp5ppw6hstetcgxkkfkfvdqh6q9zpuw7fq8qwcvnhxlvzhpnkfht3w3w06m3tq7fucsev8drkm7yzm80mzs6";
    for (addr, msk) in TESTNET11_COMMUNITY_ALLOCATIONS {
        if *addr == TETSU31_SUPERSEDED {
            assert!(!TESTNET12_COMMUNITY_ALLOCATIONS.iter().any(|(a, _)| a == addr), "a replaced address must not also be paid");
            continue;
        }
        let found = TESTNET12_COMMUNITY_ALLOCATIONS.iter().find(|(a, _)| a == addr);
        assert_eq!(found.map(|(_, m)| *m), Some(*msk), "t11 entry {addr} must carry across unchanged");
    }
    let mut seen: Vec<&str> = TESTNET12_COMMUNITY_ALLOCATIONS.iter().map(|(a, _)| *a).collect();
    seen.sort_unstable();
    let before = seen.len();
    seen.dedup();
    assert_eq!(seen.len(), before, "no address is allocated twice");
}

/// The genesis mints exactly the 10B cap, and the carve-outs are 8 collateral + 8 floats + 16
/// community + 1 main = 33 outputs.
#[test]
fn t12_genesis_mints_exactly_the_cap() {
    let utxos = genesis_premine_utxos_for(t12());
    assert_eq!(utxos.len(), 33, "8 collateral + 8 floats + 16 community + 1 main");
    let total: u64 = utxos.values().map(|e| e.amount).sum();
    assert_eq!(total, MISAKA_PREMINE_CAP_SOMPI, "testnet-12 mints exactly the 10B cap");
}

/// **The collateral the premine carves is the one the card derives.** The pin exists because the
/// premine cannot run the derivation (it needs the class profiles); this is what keeps the two
/// honest, and it is the test that would have caught the 38,889,673 MSK figure as a 24× overshoot.
#[test]
fn t12_bond_collateral_matches_the_card() {
    let p = Params::from(t12());
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("testnet-12 is a ConsensusV2 network") };
    let declared: Vec<u64> = bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            PalwConsensusObjectV2::BondRegistered { collateral, .. } => Some(*collateral),
            _ => None,
        })
        .collect();
    assert_eq!(declared.len(), 8, "eight genesis bonds");
    for c in &declared {
        assert_eq!(*c, PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI, "the card declares exactly what the premine carves (audit C-08)");
        assert_eq!(*c, 6_008_818_407_600, "60,088.18407600 MSK: the fraud a bond reachable claims authorize (ADR-0151 D1)");
    }
    // And the premine really holds it.
    let utxos = genesis_premine_utxos_for(t12());
    for card in PALW_RC_GENESIS_BONDS {
        let held = utxos.get(&premine_outpoint(card.premine_index)).map(|e| e.amount);
        assert_eq!(held, Some(PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI), "bond {} holds its declared collateral", card.premine_index);
    }
}

/// **No model class declares an economic share.** ADR-0137 made a share a RESULT — past
/// `palw_work_target` the draw is `MAX · min(1, CCU/W₀)` and the class target is not read — so a
/// genesis that handed a tier 489‰ would be publishing a policy nothing enforces, and would break
/// ADR-0144's "registering a model must not change an unrelated model's economics".
#[test]
fn no_model_class_declares_a_share() {
    let p = Params::from(t12());
    assert_eq!(p.palw_work_target, Some(ForkActivation::always()), "the work target is in force from DAA 0");
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!() };
    let floor = bundle.base_class_id;
    let min_share = bundle.state.min_grantable_share_permille();
    let mut models = 0;
    for o in bundle.genesis_objects.iter() {
        if let PalwConsensusObjectV2::ClassRegistered { class_id, share_permille, .. } = o {
            if *class_id == floor {
                continue;
            }
            models += 1;
            assert_eq!(
                *share_permille, min_share,
                "a model row declares the minimum grantable share and never an allocation: block rights come from \
                 verified work (Final -> CanonicalWork -> execution quanta -> the lane's one permit a round)"
            );
        }
    }
    assert_eq!(models, 2, "the two held rows an artifact exists for");
}

/// **The two held rows, at the widths an artifact can serve** — and the dense one is the class the
/// fleet is already mining, so it is a genesis class here rather than a post-genesis registration.
#[test]
fn the_held_rows_are_the_fleets_classes() {
    let dense = kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v7(PALW_T12_DENSE_N_CTX)
        .expect("the held dense row derives at 2M");
    let hybrid = kaspa_consensus_core::palw_context_ladder::palw_qwen36_context_row_profile_v7(PALW_T12_HYBRID_N_CTX)
        .expect("the held hybrid row derives at 512");
    assert_eq!(PALW_T12_DENSE_N_CTX, 2_097_152, "ADR-0103's widest context");
    // `/root/palw-class/qwen25-1.5b-a16-2m.class-registration.json` on all four hosts:
    // "Qwen2.5-1.5B A16 graph-v7@2097152".
    let hex = |h: kaspa_consensus_core::Hash64| h.as_bytes().iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(
        hex(dense.shape_profile_id()),
        "74c67e63d9c03daa05880c5d8a47b354ca20e952b1a2d49c107abe14f890a9c50790371bb715c7cea33ae8ac9213a3a63da409070cb2c98b8e861598db902f7a",
        "the dense row is the class the fleet mines"
    );
    let p = Params::from(t12());
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!() };
    for id in [dense.shape_profile_id(), hybrid.shape_profile_id()] {
        assert!(
            bundle.genesis_objects.iter().any(|o| matches!(o, PalwConsensusObjectV2::ClassRegistered { class_id, .. } if *class_id == id)),
            "every held row this preset names is registered at genesis"
        );
        assert!(bundle.class_catalog_root != kaspa_consensus_core::Hash64::default());
    }
    // A held class is one that registered a held map, and both of these did — which is the fact
    // that lifts the `n_ctx × layer_count` product ceiling and is the ONLY reason 2M derives.
    assert!(kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&dense));
    assert!(kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&hybrid));
}

/// **Every rule this binary can carry is in force at DAA 0, and the three exceptions are named.**
#[test]
fn every_rule_is_in_force_from_genesis() {
    let p = Params::from(t12());
    // The regenesis's whole point: no PALW fence is scheduled at a height. The one exception is
    // ADR-0065 D1's maturity window, which cannot have elapsed at block zero.
    let scheduled = |f: Option<ForkActivation>| {
        f.filter(|a| *a != ForkActivation::never() && *a != ForkActivation::always()).map(|a| a.daa_score())
    };
    assert_eq!(
        scheduled(p.palw_bond_maturity.map(|m| m.activation)),
        Some(PALW_T12_BOND_MATURITY_WINDOW_DAA),
        "the maturity window is armed AT its own window, the only scheduled height on this network"
    );
    for (name, f) in [
        ("palw_context_ladder", p.palw_context_ladder),
        ("palw_held_context", p.palw_held_context),
        ("palw_token_lift", p.palw_token_lift),
        ("palw_model_registry", p.palw_model_registry),
        ("palw_work_target", p.palw_work_target),
        ("palw_canonical_work", p.palw_canonical_work),
        ("palw_admission_independence", p.palw_admission_independence),
        ("palw_artifact_root_ownership", p.palw_artifact_root_ownership),
        ("palw_epoch_budget_release", p.palw_epoch_budget_release),
        ("palw_seat_gate_possession", p.palw_seat_gate_possession),
        ("palw_class_receipt_window", p.palw_class_receipt_window),
        ("palw_execution_quanta", p.palw_execution_quanta),
        ("palw_objective_offence", p.palw_objective_offence),
        ("palw_verification_v2", p.palw_verification_v2),
        ("palw_verification_s3", p.palw_verification_s3),
        ("palw_verification_s2", p.palw_verification_s2),
        ("palw_heartbeat_transparent", p.palw_heartbeat_transparent),
        ("palw_share_growth_final", p.palw_share_growth_final),
        ("palw_operator_id_unique", p.palw_operator_id_unique),
        ("palw_epoch_boundary_budget", p.palw_epoch_boundary_budget),
        ("palw_validator_payout_bounds", p.palw_validator_payout_bounds),
        ("palw_prompt_ids_merkle", p.palw_prompt_ids_merkle),
        ("palw_signature_contexts_v2", p.palw_signature_contexts_v2),
    ] {
        assert_eq!(f, Some(ForkActivation::always()), "{name} must be in force from DAA 0 on testnet-12");
    }
    // The three a build cannot or must not carry — asserted so "we meant to leave that one" and
    // "we forgot that one" do not look identical in a diff.
    assert!(p.palw_inactivity_leak.is_none(), "validate_palw_v2 refuses the retired leak");
    assert!(p.palw_frontier_provenance.is_none(), "ADR-0065 D2 is unimplementable inside the state fold");
    assert!(p.palw_beacon_fold.is_none(), "PALW block production has no beacon to fold");
    assert!(p.palw_fp_decode_rules.is_none(), "this build carries neither half of ADR-0082 D10/D11");
    assert!(p.palw_fp_decode_constraint.is_none(), "this build carries no constraint automaton");
    assert!(p.palw_shard_licensing.is_none(), "refused beside palw_admission_independence (ADR-0147)");
    // A ConsensusV2 network activates no V1 PALW proof-of-work.
    assert_eq!(p.pow_palw_activation, ForkActivation::never());
    assert_eq!(p.pow_palw_ollama_activation, ForkActivation::never());
    assert_eq!(p.pow_blake2b_sha3_activation, ForkActivation::never());
    // The EVM lane's four future rules, armed by name.
    assert_eq!(p.evm_gas_pool_v2_activation_daa_score, 0);
    assert_eq!(p.evm_f002_withdraw_cap_activation_daa_score, 0);
    assert_eq!(p.evm_f003_mldsa_verify_activation_daa_score, 0);
    assert_eq!(p.evm_typed_receipt_root_activation_daa_score, 0);
    // The global scarcity every model shares: one permit a round, one span a DAA, no widening.
    let lane = p.palw_execution_lane.expect("the execution lane is armed");
    assert_eq!(lane.permits_per_round, 1, "one execution permit a round is the ceiling on block rights");
    assert_eq!(lane.schedule_span_daa, 1);
    assert!(!lane.short_span.is_used(), "a chain born at the short span has no second height");
}

/// testnet-12 keeps testnet-11's peer port, by the operator's decision at the regenesis.
#[test]
fn t12_keeps_t11_peer_port() {
    assert_eq!(t12().default_p2p_port(), 26311);
    assert_eq!(NetworkId::with_suffix(NetworkType::Testnet, 11).default_p2p_port(), 26311);
}

/// **The fleet's own command lines keep working across the regenesis.**
///
/// Every live node names three premine outpoints on its command line — its bond's collateral index,
/// its fee float index, and (through `--palw-producer-class`) the class it produces for. If any of
/// them moved, the switch would look like a working deployment and every submission would fail: a
/// `--palw-fee-outpoint` that names nothing funds no lifecycle transaction, and a producer whose
/// escrow never releases is the closed loop the fee floats exist to open.
///
/// Measured off the hosts on 2026-09-22: bond-1 pairs with fee outpoint 42 … bond-6 with 47.
#[test]
fn the_fleets_premine_outpoints_are_unchanged() {
    let utxos = genesis_premine_utxos_for(t12());
    // The floats sit after the main wallet, one per card, in card order.
    for (position, card) in PALW_RC_GENESIS_BONDS.iter().enumerate() {
        let float_index = MAIN_PREMINE_INDEX + 1 + position as u32;
        let float = utxos.get(&premine_outpoint(float_index)).map(|e| e.amount);
        assert_eq!(float, Some(PALW_RC_BOND_FEE_FLOAT_SOMPI), "bond {} float at outpoint {float_index}", card.premine_index);
        // And the collateral sits at the card's own declared index, which is its bond identity.
        let collateral = utxos.get(&premine_outpoint(card.premine_index)).map(|e| e.amount);
        assert_eq!(collateral, Some(PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI), "bond {} collateral", card.premine_index);
    }
    // The six the fleet actually runs, spelled out so the pairing is readable rather than derived.
    for (bond_index, fee_index) in [(1u32, 42u32), (2, 43), (3, 44), (4, 45), (5, 46), (6, 47)] {
        let position = PALW_RC_GENESIS_BONDS
            .iter()
            .position(|c| c.premine_index == bond_index)
            .expect("the fleet's bond is a genesis card");
        assert_eq!(
            MAIN_PREMINE_INDEX + 1 + position as u32,
            fee_index,
            "bond {bond_index} must still pair with fee outpoint {fee_index} — the host command lines name it"
        );
    }
}
