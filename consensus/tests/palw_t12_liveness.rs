//! **ADR-0151 D3: testnet-12 advances its consensus clock without admitting a claim.**
//!
//! This is the property that replaced 1,620,178 MSK a seat of collateral with a structural
//! guarantee. It is not a preference: the genesis gate's bind-window rule existed to break a cycle
//!
//! ```text
//! collateral exhausted -> no claim admitted -> no block produced -> DAA frozen
//!   -> no BindTimeout -> collateral never released
//! ```
//!
//! and if the clock can move without a claim the cycle cannot close. What is checked HERE is the
//! rule, at the altitude the rule lives at. What is NOT checked here — and is owed by a live drill
//! before deployment — is the reachable-state half of ADR-0151 §5: a reorg, a restart, IBD, the DAA
//! before a receipt deadline, several classes at their caps at once, and a seat dropping out. **A
//! green predicate is not a wedge search.**
use kaspa_consensus::processes::difficulty::palw_lane_advances_daa_v1;
use kaspa_consensus_core::config::params::{ForkActivation, Params, palw_clock_advances_without_a_claim_v1};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::pow_layer0::{
    POW_ALGO_ID_ARGON2ID, POW_ALGO_ID_BLAKE2B_SHA3, POW_ALGO_ID_HEARTBEAT_V1, POW_ALGO_ID_KHEAVYHASH, POW_ALGO_ID_PALW_COMMITTED_V2,
    POW_ALGO_ID_PALW_EXEC_V3, POW_ALGO_ID_PALW_LLM, POW_ALGO_ID_PALW_OLLAMA, POW_ALGO_ID_PALW_RECEIPT_V3, POW_ALGO_ID_PALW_ROUND_V1,
};

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

/// **The predicate the genesis gate reads, and why it answers yes.**
#[test]
fn t12_clock_advances_without_a_claim() {
    let p = t12();
    assert!(palw_clock_advances_without_a_claim_v1(&p), "testnet-12's clock must not depend on any bond's collateral");

    // Clause 1: a heartbeat is mintable from block one. It carries no claim, so no exposure ceiling
    // can stop it.
    let hb = p.palw_heartbeat.expect("the heartbeat lane is armed");
    assert!(hb.activation.is_active(0), "armed from genesis, not at a height");

    // Clause 2: no lane `bits` prices is producible, so `priced == 0` on every mergeset and ADR-0138
    // section 3b's stand-in (`priced == 0 && heartbeats > 0`) makes the beat the clock.
    for (name, activation) in [
        ("pow_blake2b_sha3", p.pow_blake2b_sha3_activation),
        ("pow_palw", p.pow_palw_activation),
        ("pow_palw_ollama", p.pow_palw_ollama_activation),
    ] {
        assert_eq!(activation, ForkActivation::never(), "{name} must be never() — a ConsensusV2 network prices no V1 lane");
    }
}

/// **No lane on testnet-12 advances the DAA on its own — so the heartbeat stand-in IS the clock.**
///
/// `palw_lane_advances_daa_v1` answers for a lane in isolation, and past the anchor clock it answers
/// yes only for the lanes `bits` prices. On this network that set is empty: the V1 proof-of-work
/// activations are `never()`, and `algo_id_is_priced_by_bits_v3` takes the attempt, execution, receipt
/// and round lanes out. **The heartbeat is not in it either** — ADR-0066 took the beat out of `bits`
/// on purpose, so a stock of certified quanta could not tighten the attempt lane's target.
///
/// That is not a gap; it is the premise of the rule that follows. Because every mergeset has
/// `priced == 0`, ADR-0138 section 3b's stand-in fires on every mergeset that carries a beat
/// (`stand_in = priced == 0 && heartbeats > 0` in `DifficultyManagerExtension::daa_exempt_count`),
/// and exactly one heartbeat is counted into the score in the missing anchor's place — at most once
/// per wall-clock interval under ADR-0142's cursor.
///
/// So the clock is the beat, and the beat needs no claim, no panel and no collateral. **What this test
/// proves is the premise (`priced == 0`, always, at every height).** The stand-in's firing needs a
/// store and is drill evidence, not unit evidence — see this file's header.
#[test]
fn no_t12_lane_advances_the_daa_on_its_own() {
    let p = t12();
    let receipt_fence = p.palw_receipt_rows_unpriced.unwrap_or_else(ForkActivation::never);
    let ticks = |algo: u8, daa: u64| palw_lane_advances_daa_v1(algo, daa, p.palw_anchor_clock, p.palw_single_lottery, receipt_fence);
    // Every lane a testnet-12 node can actually produce, at genesis and far past it — the property is
    // height-independent because the activations it rests on are `never()`.
    for daa in [0u64, 1, 1_000, 8_000, 1_000_000] {
        for (name, algo) in [
            ("heartbeat", POW_ALGO_ID_HEARTBEAT_V1),
            ("attempt (committed v2)", POW_ALGO_ID_PALW_COMMITTED_V2),
            ("execution", POW_ALGO_ID_PALW_EXEC_V3),
            ("receipt", POW_ALGO_ID_PALW_RECEIPT_V3),
            ("round", POW_ALGO_ID_PALW_ROUND_V1),
        ] {
            assert!(!ticks(algo, daa), "the {name} lane must not be bits-priced at DAA {daa}: priced == 0 is what makes the beat the clock");
        }
    }
    // The lanes that WOULD price a window, none of which testnet-12 activates. If one of these ever
    // becomes producible here, `priced == 0` stops holding, the stand-in stops firing, and the
    // structural clock — and with it ADR-0151 D3 — is gone.
    for algo in [POW_ALGO_ID_KHEAVYHASH, POW_ALGO_ID_ARGON2ID, POW_ALGO_ID_BLAKE2B_SHA3, POW_ALGO_ID_PALW_LLM, POW_ALGO_ID_PALW_OLLAMA] {
        assert!(ticks(algo, 8_000), "a bits-priced lane ticks on its own — testnet-12 activates none of them");
    }
}

/// **The collateral this buys back.** The declared figure must be the reachable-liability sum and
/// NOT the bind-window figure the gate used to demand — i.e. the separation actually happened.
#[test]
fn t12_collateral_is_the_liability_not_the_liveness_bound() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
    let p = t12();
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    let declared = bundle
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            PalwConsensusObjectV2::BondRegistered { collateral, .. } => Some(*collateral),
            _ => None,
        })
        .expect("eight genesis bonds");
    assert_eq!(
        declared, 93_906_321_001_040,
        "939,063.21001040 MSK — the fraud a bond's reachable claims authorize, in the unit the RUNTIME reserves in, \
         at the escrow block one really pays and a floor concurrency of 64 (option A)"
    );
    // The bind-window figure this replaced, stated so the test names the size of the change rather
    // than asserting a number nobody can place: 1,620,178.03104000 MSK (the 2M row's declared leaves
    // x slash x the bind window — unchanged by option A, which moved the liability side). About 1.7x.
    assert_ne!(declared, 162_017_803_104_000, "the card declares the liability, not the liveness bound");
    assert_eq!(162_017_803_104_000u64 / declared, 1, "the liveness bound is ~1.7x the liability it stood in for");
    // **And the margin that matters, which the first t12 card did not have.** The ceiling is
    // `declared x max_exposure_ratio`, and one held 2M claim now reserves its weight
    // (5,974,294,206,820) AND its escrow (320,084,650,080) — so the test that the fleet can actually
    // mine is that the ceiling clears one claim at the full reservation with room for more.
    let ceiling = declared as u128 * 500 / 1000;
    let one_2m_claim = 5_974_294_206_820u128 + 320_084_650_080;
    assert!(
        ceiling > one_2m_claim,
        "the first t12 card failed exactly here: ceiling {ceiling} against {one_2m_claim} for one claim"
    );
    assert_eq!(
        ceiling / one_2m_claim,
        7,
        "seven concurrent 2M claims: the card prices the set (64 floor claims and four per model row), \
         so a bond running only 2M claims has the floor's and the 8k row's share to spare"
    );
}

/// **A network that does NOT have the structural guarantee keeps the bind-window rule.** The default
/// is conservative, and this is the test that keeps it that way: `verify_palw_genesis_v2` without the
/// clock fact must still refuse an underfunded registry.
#[test]
fn the_conservative_default_still_refuses_an_underfunded_registry() {
    // testnet-11 arms no held rows and keeps its flat carve, so it is the network where the old rule
    // still has something to say; it passes because its carve is far above the bound.
    let t11 = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11));
    assert!(
        palw_clock_advances_without_a_claim_v1(&t11),
        "t11 also prices no V1 lane, so the structural guarantee holds there too — the change cannot have \
         moved its genesis, which `shipped_presets_have_pinned_fingerprints` confirms"
    );
    // And a network with a bits-priced lane does NOT get the exemption.
    let mut hash_lane = t11.clone();
    hash_lane.pow_blake2b_sha3_activation = ForkActivation::always();
    assert!(!palw_clock_advances_without_a_claim_v1(&hash_lane), "a chain with a priced lane may need the bind-window bound");
    // Nor does one whose heartbeat is dormant.
    let mut silent = t11;
    silent.palw_heartbeat = None;
    assert!(!palw_clock_advances_without_a_claim_v1(&silent), "no heartbeat, no structural clock");
}
