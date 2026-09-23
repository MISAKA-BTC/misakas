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

/// The community table is 17 entries summing to 858M MSK, every address distinct, and it CONTAINS
/// testnet-11's thirteen — a regenesis carries the allocations across, it does not restart them.
#[test]
fn t12_community_table_is_t11_plus_four() {
    let total: u64 = TESTNET12_COMMUNITY_ALLOCATIONS.iter().map(|(_, msk)| *msk).sum();
    assert_eq!(
        TESTNET12_COMMUNITY_ALLOCATIONS.len(),
        17,
        "13 carried over + maruko + nyanmi-1828 + tetsu31's LLM address + the operator's 2026-09-24 100M"
    );
    assert_eq!(total, 858_000_000, "858M MSK");
    const OPERATOR_0924: &str =
        "misakatest:qffaadrfjpt9gy3705xhr2n6085767w290lgf0xd55nrj8px2lk8cj8w34scu4y7l5avauhul3lu9apzc6vugkeu3jhkltgrvfk4m6emz4hjtsfy";
    assert_eq!(TESTNET12_COMMUNITY_ALLOCATIONS.last(), Some(&(OPERATOR_0924, 100_000_000)), "appended last, never inserted");
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

/// The genesis mints exactly the 10B cap, and the carve-outs are 8 collateral + 8 floats + 17
/// community + 1 main = 34 outputs.
#[test]
fn t12_genesis_mints_exactly_the_cap() {
    let utxos = genesis_premine_utxos_for(t12());
    assert_eq!(utxos.len(), 34, "8 collateral + 8 floats + 17 community + 1 main");
    let total: u64 = utxos.values().map(|e| e.amount).sum();
    assert_eq!(total, MISAKA_PREMINE_CAP_SOMPI, "testnet-12 mints exactly the 10B cap");
}

/// **The collateral the premine carves is the one the card derives.** The pin exists because the
/// premine cannot run the derivation (it needs the class profiles); this is what keeps the two
/// honest, and it is the test that would have caught the 38,889,673 MSK figure as a 24× overshoot.
///
/// It would NOT have caught the figure that actually shipped and wedged the fleet: 60,088.18 MSK was
/// self-consistent between card and premine and wrong in its UNIT. `t12_producer_exposure_measured`
/// is the test for that, and it compares against the live fleet's own log line rather than against
/// another copy of the same derivation.
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
        assert_eq!(
            *c, 93_906_321_001_040,
            "939,063.21001040 MSK: the fraud a bond's reachable claims authorize, in the unit the RUNTIME reserves in \
             (ADR-0151 D1), at the escrow block one really pays and a floor concurrency of 64 (option A). The \
             516,429.79663480 MSK before it priced the escrow at a subsidy no block pays (1,200x short); the \
             60,088.18407600 MSK before that wedged every producer of the first t12 fleet shut at produced=0."
        );
    }
    // And the premine really holds it.
    let utxos = genesis_premine_utxos_for(t12());
    for card in PALW_T12_GENESIS_BONDS {
        let held = utxos.get(&premine_outpoint_for(t12(), card.premine_index)).map(|e| e.amount);
        assert_eq!(held, Some(PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI), "bond {} holds its declared collateral", card.premine_index);
    }
}

/// **Option A is live on the network that ships it: a claim reserves its escrow on its bond from DAA 0.**
///
/// The bundle's copy of the height is borsh-skipped and mirrored from `palw_audit_2026_09_23` when the
/// bundle is assembled, so a preset that armed the fence and a bundle that did not mirror it would
/// reserve only the weight — the 1,200×-short collateral this regenesis exists to correct.
/// `validate_palw_v2` refuses the two apart; this pins that they are together on testnet-12, and
/// absent everywhere the fence is.
#[test]
fn t12_reserves_the_escrow_from_genesis() {
    let p = Params::from(t12());
    assert_eq!(p.palw_audit_2026_09_23, Some(ForkActivation::always()), "the audit's fence is armed from DAA 0");
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    assert_eq!(bundle.state.escrow_backed_exposure_from_daa(), Some(0), "the bundle reserves the escrow from DAA 0");
    // A claim accepted at any height carries its escrow into the reservation…
    assert_eq!(bundle.state.claim_escrow_reservation_v1(0, 320_084_650_080), 320_084_650_080);
    // …and testnet-11, where the fence is dormant, reserves only the weight.
    let t11 = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11));
    let PalwConsensusMode::ConsensusV2(b11) = &t11.palw_consensus_mode else { panic!("testnet-11 is ConsensusV2") };
    assert_eq!(b11.state.escrow_backed_exposure_from_daa(), None);
    assert_eq!(b11.state.claim_escrow_reservation_v1(u64::MAX, 320_084_650_080), 0);
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

/// **The two held rows are both the dense family's, at 8,192 and at 2M** (operator decision
/// 2026-09-23): the 2M row is the class the fleet was already mining, and the 8k row is the one a
/// fleet host can actually produce at. The held hybrid row is gone — its map is wrong for Qwen3.6's
/// geometry, so it would be a genesis class nobody can produce for.
#[test]
fn the_held_rows_are_the_fleets_classes() {
    let dense = kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v7(PALW_T12_DENSE_N_CTX)
        .expect("the held dense row derives at 2M");
    let narrow = kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v7(PALW_T12_NARROW_DENSE_N_CTX)
        .expect("the held dense row derives at 8,192");
    assert_eq!(PALW_T12_DENSE_N_CTX, 2_097_152, "ADR-0103's widest context");
    assert_eq!(PALW_T12_NARROW_DENSE_N_CTX, 8_192, "the dense ladder's third rung");
    // `/root/palw-class/qwen25-1.5b-a16-2m.class-registration.json` on all four hosts:
    // "Qwen2.5-1.5B A16 graph-v7@2097152"; and the 8k sidecar `palw-class manifest` wrote on
    // 5.104.81.23: "Qwen/Qwen2.5-1.5B/graph-v7@8192".
    let hex = |h: kaspa_consensus_core::Hash64| h.as_bytes().iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(
        hex(dense.shape_profile_id()),
        "74c67e63d9c03daa05880c5d8a47b354ca20e952b1a2d49c107abe14f890a9c50790371bb715c7cea33ae8ac9213a3a63da409070cb2c98b8e861598db902f7a",
        "the 2M row is the class the fleet mines"
    );
    assert_eq!(
        hex(narrow.shape_profile_id()),
        "ebf44d0aa09ff7d1310a7855ab4005c275cdce557e32c269b0f3a984ea80ca73ad1ea0c9b1c0539c8ae04abb5fe24399e67e05bb0895a3dee82253e772246d01",
        "the 8k row is the class the 8k artifact's sidecar names"
    );
    let p = Params::from(t12());
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!() };
    let registered: Vec<_> = bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, .. } if *class_id != bundle.base_class_id => Some(*class_id),
            _ => None,
        })
        .collect();
    assert_eq!(
        registered,
        vec![narrow.shape_profile_id(), dense.shape_profile_id()],
        "exactly the two dense rows, the 8k row first — and no hybrid row"
    );
    assert!(bundle.class_catalog_root != kaspa_consensus_core::Hash64::default());
    // A held class is one that registered a held map, and both of these did — which is the fact
    // that lifts the `n_ctx × layer_count` product ceiling and is the ONLY reason 2M derives.
    assert!(kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&dense));
    assert!(kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&narrow));
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
        // The 2026-09-23 economic audit's fixes, and the one testnet-11 rule the regenesis had
        // dropped by omission (ADR-0065 D4) — both asserted so that omission cannot recur silently.
        ("palw_audit_2026_09_23", p.palw_audit_2026_09_23),
        ("palw_unavailable_abstains", p.palw_unavailable_abstains),
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

/// **The fleet's premine INDICES are unchanged across the regenesis — on testnet-12's own txid.**
///
/// Every live node names three premine outpoints on its command line — its bond's collateral index,
/// its fee float index, and (through `--palw-producer-class`) the class it produces for. The indices
/// keep their meaning (bond-1 pairs with fee outpoint 42 … bond-6 with 47, measured off the hosts
/// on 2026-09-22), but since the 2026-09-24 replay separation they sit on
/// `premine_txid_for(testnet-12)`, not on the sentinel every other network (and every private
/// testnet-12 instance) used: a command line naming `<sentinel>:<index>` names nothing here, and must
/// be rewritten to the new txid. Nothing on this chain sits on the sentinel at all.
#[test]
fn the_fleets_premine_indices_are_unchanged_on_t12s_own_txid() {
    let utxos = genesis_premine_utxos_for(t12());
    let sentinel = premine_outpoint(0).transaction_id;
    assert_ne!(premine_txid_for(t12()), sentinel, "testnet-12's premine is its own name");
    assert!(utxos.keys().all(|o| o.transaction_id != sentinel), "no testnet-12 genesis output sits on the shared sentinel");
    // The floats sit after the main wallet, one per card, in card order.
    for (position, card) in PALW_T12_GENESIS_BONDS.iter().enumerate() {
        let float_index = MAIN_PREMINE_INDEX + 1 + position as u32;
        let float = utxos.get(&premine_outpoint_for(t12(), float_index)).map(|e| e.amount);
        assert_eq!(float, Some(PALW_RC_BOND_FEE_FLOAT_SOMPI), "bond {} float at outpoint {float_index}", card.premine_index);
        // And the collateral sits at the card's own declared index, which is its bond identity.
        let collateral = utxos.get(&premine_outpoint_for(t12(), card.premine_index)).map(|e| e.amount);
        assert_eq!(collateral, Some(PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI), "bond {} collateral", card.premine_index);
    }
    // The eight the fleet runs after the regenesis (bond 0 on ibm beside bond 1, the re-keyed bond 7
    // beside bonds 2–5), spelled out so the pairing is readable rather than derived.
    for (bond_index, fee_index) in [(0u32, 41u32), (1, 42), (2, 43), (3, 44), (4, 45), (5, 46), (6, 47), (7, 48)] {
        let position = PALW_T12_GENESIS_BONDS
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

/// **The genesis card's dense root is the committed measurement, and it belongs to the class the
/// card registers.**
///
/// `inventory_root_of_class` reads the manifest POSITIONALLY — keying it by class id would need that
/// id as a second literal, which is the thing the manifest exists to remove. So the pairing is
/// asserted here instead: the class id beside the root in the committed file must be the id the
/// profile this card registers derives. A manifest regenerated over a different artifact, or a card
/// that moved to a different `n_ctx`, fails here rather than registering a root for the wrong class.
#[test]
fn t12_genesis_reads_its_root_from_the_committed_manifest() {
    use kaspa_consensus_core::config::class_manifest_const_v1 as manifest;

    let p = Params::from(t12());
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    for (name, text, n_ctx, pinned) in [
        ("2M", manifest::QWEN25_A16_2M_MANIFEST_V1, PALW_T12_DENSE_N_CTX, PALW_T12_GENESIS_QWEN25_A16_2M_ARTIFACT_ROOT),
        ("8k", manifest::QWEN25_A16_8K_MANIFEST_V1, PALW_T12_NARROW_DENSE_N_CTX, PALW_T12_GENESIS_QWEN25_A16_8K_ARTIFACT_ROOT),
    ] {
        let read_root = manifest::inventory_root_of_class(text, 1);
        let read_class = manifest::class_id_of_class(text, 1);

        // The card registers exactly what it read.
        assert_eq!(pinned, read_root, "{name}: the genesis constant is the manifest's root, not a copy of it");

        // And the row the card builds is the class that manifest row is about.
        let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v7(
            kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
                n_ctx,
                ..kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B
            },
        )
        .expect("the graph-v7 profile projects");
        assert_eq!(
            profile.shape_profile_id(),
            read_class,
            "{name}: the manifest row this card reads is about the class this card registers"
        );

        // **The digest is in the same file and must never be the root.** The two substitutions that
        // shut two networks' dense tiers were exactly this equality holding.
        assert_ne!(pinned, manifest::artifact_digest_of(text), "{name}: the card would be registering a flat artifact digest again");

        // The registry really carries it — the card, not just the constant.
        assert!(
            bundle.genesis_objects.iter().any(|o| matches!(
                o,
                PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, .. }
                    if *class_id == read_class && *artifact_root == read_root
            )),
            "{name}: testnet-12 registers the measured root for the measured class"
        );
    }
    // Two conversions of one model are two artifacts: a shared root would give ADR-0143's one owner
    // two classes.
    assert_ne!(PALW_T12_GENESIS_QWEN25_A16_8K_ARTIFACT_ROOT, PALW_T12_GENESIS_QWEN25_A16_2M_ARTIFACT_ROOT);
}


/// **Every fence testnet-11 armed, testnet-12 arms from genesis** — the operator's question of
/// 2026-09-23, as a ledger that fails on the first regression.
///
/// The first run of this ledger found one: `palw_unavailable_abstains`, armed at 0 on t11 since
/// Relaunch 5 and dormant on t12, because it was `None` in the base preset and pass 2's zeroing has
/// nothing to visit inside a `None`. A rule the live network has run from genesis, silently dormant
/// on its successor, is exactly what a fresh genesis must not do — so the check is a test, not a
/// one-off probe. The one scheduled height (`PALW_T12_BOND_MATURITY_WINDOW_DAA`) is the single
/// allowed exception, for the reason on `palw_t12_arm_every_rule_from_genesis`.
#[test]
fn t12_arms_every_fence_t11_armed() {
    use kaspa_consensus_core::config::params::PALW_T12_BOND_MATURITY_WINDOW_DAA;
    let t11 = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11));
    let t12 = Params::from(t12());
    let on_t12: std::collections::BTreeMap<&str, Option<u64>> =
        t12.palw_fences_v1().into_iter().map(|(n, f)| (n, f.map(|a| a.daa_score()))).collect();
    let mut armed_on_t11 = 0;
    let mut regressions = Vec::new();
    for (name, fence) in t11.palw_fences_v1() {
        let Some(h11) = fence.map(|a| a.daa_score()) else { continue };
        armed_on_t11 += 1;
        match on_t12.get(name).copied().flatten() {
            Some(0) => {}
            Some(h) if h == PALW_T12_BOND_MATURITY_WINDOW_DAA => {}
            // **Not a fence on t12 — the rule it schedules is the lane's opening state.** t11's lane
            // opens at a 5-DAA span and narrows at 7,300; t12's opens AT the narrowed span with no
            // second height (`PalwExecSpanShortV1::NONE`). The exception is admitted only if that is
            // true in substance, asserted below rather than taken from the name.
            None if name == "palw_execution_lane_span_short" => {
                let (l11, l12) = (
                    t11.palw_execution_lane.as_ref().expect("t11 has an execution lane"),
                    t12.palw_execution_lane.as_ref().expect("t12 has an execution lane"),
                );
                assert_eq!(
                    l12.schedule_span_daa,
                    l11.short_span.schedule_span_daa.max(1),
                    "t12's lane must open at the span t11 narrows to at {h11}, not at t11's opening span {}",
                    l11.schedule_span_daa
                );
                assert_eq!(l12.short_span, PalwExecSpanShortV1::NONE, "and schedule no second height");
                assert_ne!(l11.schedule_span_daa, l12.schedule_span_daa, "the fixture is vacuous if t11 never narrowed");
            }
            Some(h) => regressions.push(format!("{name}: t11 {h11} -> t12 scheduled at {h}, not genesis")),
            None => regressions.push(format!("{name}: t11 {h11} -> t12 DORMANT")),
        }
    }
    assert!(armed_on_t11 >= 36, "the t11 ledger has {armed_on_t11} armed fences; the table this was written against had 36");
    assert!(regressions.is_empty(), "fences testnet-11 armed that testnet-12 does not arm from genesis:\n  {}", regressions.join("\n  "));
}

/// **The genesis free-prompt gate names exactly the classes the card registers** (the mainnet rule,
/// applied to this card on 2026-09-23), and every one of them is covered on BOTH lanes by a family
/// this build drills. Before this the shared assembly installed testnet-11's set — the graph-v3
/// hybrid and the graph-v2 dense rows, neither registered here — and the two held rows the card DOES
/// register were two kernels short of any family: no free-prompt lane, and no way to add a held
/// hybrid row past genesis except weightless.
#[test]
fn t12_certifies_both_lanes_of_the_classes_it_registers() {
    use kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1;
    use kaspa_consensus_core::palw_e2e_adjudicability::{
        palw_rc_certified_families_v1, palw_rc_court_e2e_root_v1, palw_rc_fp_certified_families_v1,
    };
    let p = Params::from(t12());
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!() };
    let registered: std::collections::BTreeSet<_> = bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, .. } => Some(*class_id),
            _ => None,
        })
        .collect();
    assert_eq!(registered.len(), 3, "the floor and the two held rows");
    assert_eq!(
        bundle.state.fp_certified_classes(),
        Some(&registered),
        "the free-prompt gate is the registered set — not testnet-11's, which names classes this card does not register"
    );
    // The commitment the registration gate checks the certified set against is this build's pin.
    assert_eq!(bundle.court_e2e_root, palw_rc_court_e2e_root_v1());
    // And the coverage is real on both lanes, per registered profile: both held dense rows by
    // `PALW-QWEN25-A16-V5`, the floor by `PALW-BASE-0`.
    let attempt = palw_rc_certified_families_v1();
    let fp = palw_rc_fp_certified_families_v1();
    assert_eq!(attempt.len(), 5, "five families since 2026-09-23");
    assert_eq!(fp.len(), 5);
    let mut covered = 0;
    for o in &bundle.genesis_objects {
        let PalwConsensusObjectV2::ClassRegistered { class_id, admission: Some(carriage), .. } = o else { continue };
        let reachable = reachable_kernels_v1(&carriage.profile);
        for (lane, families) in [("attempt", &attempt), ("free-prompt", &fp)] {
            assert!(
                families.iter().any(|f| reachable.is_subset(&f.kernel_ids)),
                "genesis class {class_id} is covered by no {lane}-lane family"
            );
        }
        covered += 1;
    }
    assert_eq!(covered, 2, "both held rows carry a profile and are covered");
}

/// **Every root testnet-12's genesis registers is read out of a committed measurement — all of them,
/// keyed by the class that registers it.**
///
/// Three instances of one substitution (the flat artifact digest, or the mapping's own root, where the
/// operand-inventory root belonged) is a mechanism failing rather than three mistakes:
///
/// * testnet-11's dense tier: `artifact_digest()` pinned as the root → zero blocks;
/// * testnet-12's dense tier: the same, over a byte-identical artifact → zero blocks;
/// * testnet-12's HELD HYBRID row (found 2026-09-23, the day this artifact's sidecar was first
///   derived): `PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT` — the mapping's own root, and the right value
///   for testnet-11's `graph-v3` row — pinned into a `graph-v7` row, which registers the inventory
///   root. `f4aad4fd…` where `f01230ae…` belonged. No node had failed on it only because none of them
///   holds the hybrid artifact yet.
///
/// `t12_genesis_reads_its_root_from_the_committed_manifest` asserted the dense row alone, so the
/// hybrid row was outside every check. This one enumerates the card's OWN registrations and demands a
/// committed sidecar row for each, so a class added to the card without one fails here — which is the
/// only version of this check that cannot be escaped by adding a row.
///
/// The one exception is named and substantive: the floor class's artifact is DERIVED in-process
/// (`misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1`), there is no file to measure and no
/// sidecar to commit, so its root must be the pinned `PALW_RC_GENESIS_ARTIFACT_ROOT` and nothing else.
#[test]
fn t12_genesis_roots_are_all_read_from_committed_manifests() {
    use kaspa_consensus_core::config::class_manifest_const_v1 as manifest;

    // (class id, inventory root) of every row of every committed sidecar, read through the SAME
    // const fn the genesis constants read, so the test cannot agree with a value the card does not use.
    let mut committed: Vec<(kaspa_consensus_core::Hash64, kaspa_consensus_core::Hash64)> = Vec::new();
    for (name, text) in [
        ("qwen25-1.5b-a16-2m", manifest::QWEN25_A16_2M_MANIFEST_V1),
        ("qwen25-1.5b-a16-8k", manifest::QWEN25_A16_8K_MANIFEST_V1),
        ("qwen36-35b-a3b-512", manifest::QWEN36_512_MANIFEST_V1),
    ] {
        let rows = text.matches("\"inventory_root\"").count();
        assert!(rows > 0, "{name}: a committed sidecar with no rows measures nothing");
        for occurrence in 1..=rows {
            committed.push((manifest::class_id_of_class(text, occurrence), manifest::inventory_root_of_class(text, occurrence)));
        }
    }

    let p = Params::from(t12());
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    let floor = bundle.base_class_id;
    let mut checked = 0;
    for object in &bundle.genesis_objects {
        let PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, .. } = object else { continue };
        if *class_id == floor {
            assert_eq!(
                *artifact_root, PALW_RC_GENESIS_ARTIFACT_ROOT,
                "the floor's artifact is derived in-process, so its root is the pin and never a third value"
            );
            checked += 1;
            continue;
        }
        let measured = committed.iter().find(|(id, _)| id == class_id).map(|(_, root)| *root).unwrap_or_else(|| {
            panic!(
                "genesis registers class {class_id} and no committed sidecar measures it — derive one with \
                 `palw-class manifest --network testnet-12 <artifact>` on the host that holds the file and commit it \
                 beside the card, rather than pasting a hash into the card"
            )
        });
        assert_eq!(
            *artifact_root, measured,
            "class {class_id}: the card registers a root the committed measurement of its own artifact does not give. \
             This is the substitution that shut two dense tiers and the held hybrid row — a producer holding the \
             artifact derives the measured value and is refused by ClassResolveError::ArtifactRoot"
        );
        checked += 1;
    }
    assert_eq!(checked, 3, "the floor and the two held dense rows are what this card registers");
}

/// **Replay separation (user decision, 2026-09-24): the public testnet-12 shares no premine outpoint
/// and no genesis hash with any chain that used the sentinel.** The private testnet-12 instances ran
/// the same card keys 0–7 on the sentinel txid, and the ML-DSA sighash commits to the spent outpoint
/// but to neither the network nor the genesis — so a float spend signed there was valid here. Now
/// no testnet-12 genesis output (premine or community) sits on a txid another network's genesis
/// uses, the bond identities differ from testnet-11's, and the genesis hash is not the one the
/// previous card (`f6cc9576…`) or any earlier testnet-12 minted.
#[test]
fn t12_shares_no_premine_outpoint_or_genesis_with_the_sentinel_chains() {
    let t11 = NetworkId::with_suffix(NetworkType::Testnet, 11);
    let t12_set = genesis_premine_utxos_for(t12());
    for other in [
        t11,
        NetworkId::with_suffix(NetworkType::Testnet, 10),
        NetworkId::new(NetworkType::Devnet),
        NetworkId::new(NetworkType::Simnet),
        NetworkId::new(NetworkType::Mainnet),
    ] {
        assert_eq!(premine_txid_for(other), premine_outpoint(0).transaction_id, "{other}: keeps the sentinel byte for byte");
        let set = genesis_premine_utxos_for(other);
        assert!(set.keys().all(|o| !t12_set.contains_key(o)), "{other}: no outpoint shared with testnet-12");
    }
    // The community table too: its own txid, not the `misaka-t12-community` sentinel.
    let community = testnet12_community_utxos();
    assert!(community.keys().all(|o| o.transaction_id == testnet12_community_txid()));
    assert!(!testnet12_community_txid().as_bytes().starts_with(b"misaka-t12-community"), "derived, not the sentinel");
    // Bond identities are outpoints, so testnet-12's are not testnet-11's (same indices, other txid).
    for card in PALW_T12_GENESIS_BONDS {
        assert_ne!(premine_outpoint_for(t12(), card.premine_index), premine_outpoint_for(t11, card.premine_index));
    }
    let p = Params::from(t12());
    let hex = |h: kaspa_consensus_core::Hash64| h.as_bytes().iter().map(|b| format!("{b:02x}")).collect::<String>();
    let genesis = hex(p.genesis.hash);
    for superseded in ["f6cc957686f7047d", "a8cabac47b96fe30", "d73dbf44dbae3522"] {
        assert!(!genesis.starts_with(superseded), "the public genesis is not {superseded}…");
    }
    assert_eq!(p.genesis.timestamp, 1_788_220_800_000, "2026-09-01T00:00:00Z, testnet-12's own");
    assert_ne!(p.genesis.timestamp, Params::from(t11).genesis.timestamp, "not the shared reference timestamp");
    assert_ne!(p.genesis.hash, Params::from(t11).genesis.hash);
}
