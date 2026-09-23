//! REPRO 01 — `record_round_final` credits the execution-quantum mint in RAW derived MAC-eq
//! while `PALW_EXECUTION_QUANTUM_V1` is declared in exposure pwu, and `schedule.quanta` is the
//! one collection in the rooted schedule with no bound.
//!
//! Everything below calls the SHIPPED runtime functions on the SHIPPED testnet-12 card. Nothing
//! is reimplemented. The file is read-only with respect to the tree.
//!
//! The chain of custody the tests walk, function by function, is the chain `record_round_final`
//! walks (`consensus/core/src/palw_state_v2.rs:10955-11000`):
//!
//! ```text
//!   genesis ClassRegistered.admission.profile/canonical
//!        -> palw_model_work_from_carriage_v1            (palw_model_registry_v1.rs:572)
//!        -> PalwModelWorkV1::economic_ccu_per_claim     == the rooted lifecycle row
//!        -> PalwChainStateV2::canonical_per_draw        (palw_state_v2.rs:9840)
//!        -> palw_exposure_pwu_v2(class, pwu, canonical) (palw_state_v2.rs:2051)   <-- RAW MAC-eq
//!        -> credit                                     (palw_state_v2.rs:10979)   <-- CLAMP SKIPPED
//!        -> PalwExecFinalV1.credit
//!        -> palw_execution_schedule_snapshot_v1         (palw_execution_lane_v1.rs:413)  finals: NO truncate
//!        -> palw_execution_schedule_seeded_v1           (palw_execution_lane_v1.rs:478)
//!        -> palw_execution_schedule_assign_quanta_v1    (palw_execution_lane_v1.rs:499)
//!        -> palw_execution_mint_quanta_matured_v1       (palw_execution_quanta_v1.rs:284)
//!        -> schedule.quanta  ->  write_round_schedule   (rooted state)
//! ```
//!
//! Run:
//!   cargo test -p kaspa-consensus-core --test audit_repro_01_exec_quanta_mint_has_no_byte_bound_while \
//!       -- --nocapture --test-threads=1

use std::time::Instant;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_attempt_v2::{PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2};
use kaspa_consensus_core::palw_execution_lane_v1::{
    PALW_EXEC_MAX_BONDS_PER_DOMAIN_V1, PALW_EXEC_MAX_DOMAINS_V1, PalwExecFinalV1, PalwExecLaneFoldV1, PalwExecSeedAnchorV1,
    palw_execution_credit_v1, palw_execution_schedule_assign_quanta_v1, palw_execution_schedule_seeded_v1,
    palw_execution_schedule_snapshot_v1,
};
use kaspa_consensus_core::palw_execution_quanta_v1::{
    PALW_EXECUTION_QUANTUM_V1, palw_execution_mint_quanta_v1, palw_execution_quantum_count_v1,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_model_registry_v1::palw_genesis_model_works_v1;
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClassStateV2, PalwClassStatusV2,
    PalwConsensusObjectV2 as Obj, PalwPanelSeatV2, PalwPwuRuleV2, PalwStateParamsV2, PalwTransitionExtrasV1,
    apply_palw_transition_v2_with_extras, palw_exposure_pwu_v2, palw_max_exposure_pwu_of_rule_v1, palw_operator_id_v2,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

// ---------------------------------------------------------------------------------------------
// The shipped testnet-12 card, and the three classes its genesis registers.
// ---------------------------------------------------------------------------------------------

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

struct Row {
    name: &'static str,
    class_id: Hash64,
    /// U1 — the registrant's declared step-leaf count, `PalwPwuRuleV2::DerivedV1.pwu_per_inference`.
    declared_leaves: u64,
    /// U2 — `PalwModelWorkV1::economic_ccu_per_claim`, the rooted lifecycle row's derived MAC-eq
    /// per draw. This is what `PalwChainStateV2::canonical_per_draw` returns on t12.
    derived_mac_eq: u128,
    slash_value_per_pwu: u64,
    class: PalwClassStateV2,
}

/// Every t12 genesis class, with BOTH of its measures read out of the shipped card at runtime.
fn t12_rows() -> Vec<Row> {
    let params = t12();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        panic!("testnet-12 is a ConsensusV2 network");
    };
    // The floor has no admission carriage (it is a genesis registration), so its work is derived
    // from the base-0 profile exactly as `PalwChainStateV2::base_known_draw` does it.
    let works = palw_genesis_model_works_v1(&bundle.genesis_objects);
    let base = bundle.base_class_id;
    let floor_profile = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(
        kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY,
    )
    .expect("the base-0 profile this build describes");
    let (fp, fd) = kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL;
    let floor_job = kaspa_consensus_core::palw_base0_profile::rc_job_context(&floor_profile, fp, fd);
    let floor_work = kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1(&floor_profile, &floor_job)
        .expect("the floor's work derives");

    let mut out = Vec::new();
    for object in &bundle.genesis_objects {
        let Obj::ClassRegistered { class_id, artifact_root, pwu_rule, slash_value_per_pwu, .. } = object else {
            continue;
        };
        let declared_leaves = palw_max_exposure_pwu_of_rule_v1(pwu_rule);
        let derived = if *class_id == base {
            floor_work.economic_ccu_per_claim
        } else {
            works.get(class_id).map(|w| w.economic_ccu_per_claim).expect("a model row carries a carriage")
        };
        let name = if *class_id == base {
            "BASE-0 liveness floor"
        } else if declared_leaves > 1_000_000_000 {
            "Qwen2.5-1.5B A16 graph-v7 @ n_ctx 2,097,152"
        } else {
            "Qwen3.6-35B-A3B graph-v7 @ n_ctx 512"
        };
        // The class row exactly as the fold holds it; `palw_exposure_pwu_v2` reads `pwu_rule`.
        let class = PalwClassStateV2 {
            artifact_root: *artifact_root,
            slash_value_per_pwu: *slash_value_per_pwu,
            pwu_rule: pwu_rule.clone(),
            status: PalwClassStatusV2::Active,
            registered_daa: 0,
            registrant_bond: None,
            fused_attention: false,
        };
        out.push(Row { name, class_id: *class_id, declared_leaves, derived_mac_eq: derived, slash_value_per_pwu: *slash_value_per_pwu, class });
    }
    out.sort_by_key(|r| r.declared_leaves);
    out
}

fn hex(h: &Hash64) -> String {
    faster_hex::hex_string(&h.as_byte_slice()[..8])
}

// ---------------------------------------------------------------------------------------------
// TEST 1 — the credit the mint divides is in the wrong unit, and nothing bounds the quotient.
// ---------------------------------------------------------------------------------------------

#[test]
fn repro_01_the_mint_divides_derived_mac_eq_by_a_quantum_declared_in_exposure_pwu() {
    let params = t12();

    // ---- (a) The two fences that make this reachable are armed at DAA 0 on t12. ----
    let quanta_fence = params.palw_execution_quanta.expect("t12 arms palw_execution_quanta");
    let canonical_fence = params.palw_canonical_work.expect("t12 arms palw_canonical_work");
    let lane = params.palw_execution_lane.expect("t12 arms the execution lane");
    println!("\n=== t12 runtime fences ===");
    println!("  palw_execution_quanta   active at daa 0 ? {}", quanta_fence.is_active(0));
    println!("  palw_canonical_work     active at daa 0 ? {}", canonical_fence.is_active(0));
    println!("  lane.permits_per_round  {}  max_per_mergeset {}", lane.permits_per_round, lane.max_per_mergeset);
    println!("  PALW_EXECUTION_QUANTUM_V1 = {PALW_EXECUTION_QUANTUM_V1}   (declared unit: exposure pwu)");
    assert!(quanta_fence.is_active(0), "the quantum mint must be armed at genesis or the finding is unreachable");
    assert!(canonical_fence.is_active(0), "the derived basis must be armed at genesis or the credit stays declared");
    // processor.rs:8278 — armed quanta fence => lane.execution_quantum = PALW_EXECUTION_QUANTUM_V1.
    let execution_quantum: u64 = if quanta_fence.is_active(0) { PALW_EXECUTION_QUANTUM_V1 } else { 0 };
    assert_eq!(execution_quantum, 100_000, "processor.rs:8278 hands the fold this constant on t12");
    assert!(execution_quantum > 0, "palw_state_v2.rs:10979 takes the UNCLAMPED branch whenever this is > 0");

    let seed = Hash64::from_u64_word(0xA5A5_A5A5_A5A5_A5A5);
    let final_id = Hash64::from_u64_word(0x0123_4567_89AB_CDEF);

    println!("\n=== per t12 genesis class: what record_round_final credits, and what it mints ===");
    let mut hybrid_raw_quanta = 0u32;
    let mut hybrid_declared_quanta = 0u32;
    let mut dense_raw_quanta = 0u32;
    let mut dense_declared_quanta = 0u32;

    for row in t12_rows() {
        // ---- (b) The credit, by the exact expression `record_round_final` uses. ----
        //
        // palw_state_v2.rs:10973-10976:
        //     let canonical = self.canonical_per_draw(&claim.class_id, claim.accepted_daa);
        //     let exposure  = palw_exposure_pwu_v2(class, claim.pwu, canonical);
        //
        // `canonical` is `Some(economic_ccu_per_claim)` on t12 because palw_canonical_work is
        // armed at 0 and the registry writes every genesis class a row.
        let canonical = Some(row.derived_mac_eq.min(u64::MAX as u128) as u64);
        // claim.pwu is irrelevant on the DerivedV1 arm — the runtime ignores it when canonical is
        // Some. Passing a deliberately absurd value proves the arm taken is the canonical one.
        let exposure = palw_exposure_pwu_v2(&row.class, 1, canonical);
        assert_eq!(
            exposure,
            row.derived_mac_eq.min(u64::MAX as u128) as u64,
            "{}: palw_exposure_pwu_v2 returns the RAW derived MAC-eq, not the declared leaves",
            row.name
        );
        assert_ne!(exposure, row.declared_leaves, "{}: the credit is NOT in the unit the quantum is declared in", row.name);

        // palw_state_v2.rs:10979 — the branch. With the quantum armed the clamp is skipped
        // entirely; with it dormant the credit would pass through palw_execution_credit_v1.
        let credit_armed = exposure;
        let credit_if_the_clamp_ran = palw_execution_credit_v1(exposure, row.declared_leaves);
        assert!(
            credit_armed >= credit_if_the_clamp_ran,
            "{}: the armed branch can only be >= the clamped one (it IS the unclamped value)",
            row.name
        );

        // ---- (c) The quotient. The only bound is a u32 saturation. ----
        let quanta_raw = palw_execution_quantum_count_v1(u128::from(credit_armed), u128::from(execution_quantum), seed, final_id);
        let quanta_declared =
            palw_execution_quantum_count_v1(u128::from(row.declared_leaves), u128::from(execution_quantum), seed, final_id);

        println!("\n  {} [{}…]", row.name, hex(&row.class_id));
        println!("    U1 declared leaves                       {:>22}  LEAVES", row.declared_leaves);
        println!("    U2 derived MAC-eq per draw (the credit)  {:>22}  MAC-eq", row.derived_mac_eq);
        println!("    slash_value_per_pwu                      {:>22}  sompi/pwu", row.slash_value_per_pwu);
        println!("    credit / 100,000 on the RAW unit         {:>22}  quanta", quanta_raw);
        println!("    credit / 100,000 on the DECLARED unit    {:>22}  quanta", quanta_declared);
        if quanta_declared > 0 {
            println!("    over-mint ratio                          {:>22.1}x", quanta_raw as f64 / quanta_declared as f64);
        } else {
            println!("    over-mint ratio                          {:>22}", "infinite (declared mints 0)");
        }
        if quanta_raw == u32::MAX {
            println!("    ^^ SATURATED at u32::MAX — the true quotient is {}", row.derived_mac_eq / 100_000);
        }

        if row.name.starts_with("Qwen3.6") {
            hybrid_raw_quanta = quanta_raw;
            hybrid_declared_quanta = quanta_declared;
        }
        if row.name.starts_with("Qwen2.5") {
            dense_raw_quanta = quanta_raw;
            dense_declared_quanta = quanta_declared;
        }
    }

    // ---- (d) The finding's headline numbers, asserted. ----
    println!("\n=== the exploit, asserted ===");
    println!("  Qwen3.6@512 one honest Final mints {hybrid_raw_quanta} quanta (declared unit would give {hybrid_declared_quanta})");
    println!("  Qwen2.5@2M  one honest Final mints {dense_raw_quanta} quanta (declared unit would give {dense_declared_quanta})");
    assert!(
        hybrid_raw_quanta > 1_500_000,
        "one honest Qwen3.6@512 Final must mint over 1.5 million quanta; measured {hybrid_raw_quanta}"
    );
    assert!(hybrid_declared_quanta < 1_000, "the quantum's own declared unit gives a three-digit count; measured {hybrid_declared_quanta}");
    assert!(
        hybrid_raw_quanta as u64 / hybrid_declared_quanta.max(1) as u64 > 2_800,
        "the unit mismatch is at least the floor's 2,810x; measured {}x",
        hybrid_raw_quanta / hybrid_declared_quanta.max(1)
    );
    assert_eq!(dense_raw_quanta, u32::MAX, "the dense row saturates the ONLY bound in the mint (palw_execution_quanta_v1.rs:102)");
    assert!(dense_declared_quanta > 65_536, "even the DECLARED unit puts the dense row past assign_round's 2^16 probe horizon");
}

// ---------------------------------------------------------------------------------------------
// TEST 2 — `finals` is the one collection in the same function that is never truncated,
// and the mint it feeds writes ~336 bytes per ticket into the rooted schedule.
// ---------------------------------------------------------------------------------------------

fn bond_key(v: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
}

fn a_final(domain: u64, bond: u64, claim: u64, credit: u64) -> PalwExecFinalV1 {
    PalwExecFinalV1 {
        domain: Hash64::from_u64_word(domain),
        bond: bond_key(bond),
        operator_id: Hash64::from_u64_word(0x5EA7),
        claim_id: Hash64::from_u64_word(claim),
        execution_root: Hash64::from_u64_word(claim ^ 0xE0E0),
        credit,
    }
}

#[test]
fn repro_02_the_snapshot_truncates_its_two_siblings_and_never_the_finals() {
    // 40 domains x 70 bonds each, one Final apiece: both documented caps are exceeded.
    let mut finals = Vec::new();
    let mut claim = 1u64;
    for domain in 1..=40u64 {
        for bond in 1..=70u64 {
            finals.push(a_final(domain, domain * 1_000 + bond, claim, 1_000_000));
            claim += 1;
        }
    }
    let input_finals = finals.len();
    let snapshot = palw_execution_schedule_snapshot_v1(7, &finals);

    println!("\n=== palw_execution_schedule_snapshot_v1 — the three collections it returns ===");
    println!("  input finals                          {input_finals}");
    println!("  domains  (cap PALW_EXEC_MAX_DOMAINS_V1 = {PALW_EXEC_MAX_DOMAINS_V1})   -> {}", snapshot.domains.len());
    let widest = snapshot.domains.iter().map(|d| d.bonds.len()).max().unwrap_or(0);
    println!("  widest domain's bonds (cap PALW_EXEC_MAX_BONDS_PER_DOMAIN_V1 = {PALW_EXEC_MAX_BONDS_PER_DOMAIN_V1}) -> {widest}");
    println!("  finals   (cap  — NONE —)                 -> {}", snapshot.finals.len());

    assert_eq!(snapshot.domains.len(), PALW_EXEC_MAX_DOMAINS_V1, "palw_execution_lane_v1.rs:431 truncates domains");
    assert!(widest <= PALW_EXEC_MAX_BONDS_PER_DOMAIN_V1, "palw_execution_lane_v1.rs:453 truncates bonds");
    assert_eq!(
        snapshot.finals.len(),
        input_finals,
        "palw_execution_lane_v1.rs:456 passes `finals` through whole — the ONE collection in this \
         function with no truncate, and it is the one the mint reads"
    );
}

#[test]
fn repro_03_the_rooted_schedule_costs_about_336_bytes_per_minted_ticket() {
    // One Final whose credit mints exactly 2,000 tickets — small enough to mint here, large
    // enough that the per-ticket cost is not swamped by the schedule's fixed header.
    let n: u64 = 2_000;
    let one = a_final(1, 1, 1, n * PALW_EXECUTION_QUANTUM_V1);
    let snapshot = palw_execution_schedule_snapshot_v1(7, &[one]);
    let anchor = PalwExecSeedAnchorV1 { span: 6, block: Hash64::from_u64_word(6), execution_key: Hash64::from_u64_word(0xBEEF) };
    let mut schedule = palw_execution_schedule_seeded_v1(&snapshot, &anchor, 1, Hash64::from_u64_word(0xF0F0));

    let empty_bytes = borsh::to_vec(&schedule).expect("a schedule serializes").len();
    palw_execution_schedule_assign_quanta_v1(&mut schedule, PALW_EXECUTION_QUANTUM_V1, 0);
    let full_bytes = borsh::to_vec(&schedule).expect("a schedule serializes").len();

    assert_eq!(schedule.quanta.len() as u64, n, "the mint issued exactly credit/quantum tickets");
    let per_ticket = (full_bytes - empty_bytes) as f64 / n as f64;
    println!("\n=== rooted bytes of one span's schedule ===");
    println!("  schedule with 0 quanta      {empty_bytes:>12} bytes");
    println!("  schedule with {n} quanta  {full_bytes:>12} bytes");
    println!("  per minted ticket           {per_ticket:>12.1} bytes");

    // What the same expression costs for the class the t12 genesis actually registers.
    let hybrid = t12_rows().into_iter().find(|r| r.name.starts_with("Qwen3.6")).expect("the hybrid row");
    let hybrid_quanta = palw_execution_quantum_count_v1(
        hybrid.derived_mac_eq,
        u128::from(PALW_EXECUTION_QUANTUM_V1),
        Hash64::default(),
        Hash64::default(),
    );
    let hybrid_bytes = hybrid_quanta as f64 * per_ticket;
    println!("\n  Qwen3.6@512, ONE honest Final:");
    println!("    quanta minted                {hybrid_quanta:>12}");
    println!("    rooted bytes                 {hybrid_bytes:>12.0} = {:.0} MB", hybrid_bytes / 1e6);

    assert!(per_ticket > 300.0 && per_ticket < 400.0, "a ticket costs ~336 rooted bytes; measured {per_ticket:.1}");
    assert!(
        hybrid_bytes > 500e6,
        "one honest hybrid Final writes over half a gigabyte into the rooted round_schedules row; computed {hybrid_bytes:.0} bytes"
    );
}

// ---------------------------------------------------------------------------------------------
// TEST 4 — assign_round's 2^16 probe horizon: the mint is super-quadratic past 65,536 tickets.
// ---------------------------------------------------------------------------------------------

#[test]
fn repro_04_the_mint_falls_off_a_cliff_at_the_2_16_probe_horizon() {
    let seed = Hash64::from_u64_word(0xC0FFEE);
    let mut timings: Vec<(u64, f64)> = Vec::new();
    println!("\n=== palw_execution_mint_quanta_v1 wall time vs ticket count (debug profile) ===");
    for n in [16_000u64, 32_000, 64_000, 70_000, 85_000] {
        let one = a_final(1, 1, 1, n * PALW_EXECUTION_QUANTUM_V1);
        let t0 = Instant::now();
        let issued = palw_execution_mint_quanta_v1(&[one], seed, u128::from(PALW_EXECUTION_QUANTUM_V1), 0);
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(issued.len() as u64, n, "every ticket is issued");
        println!("  n = {n:>7}  ->  {ms:>12.1} ms   ({:.1} us/ticket)", ms * 1000.0 / n as f64);
        timings.push((n, ms));
    }

    let (n_small, ms_small) = timings[2]; // 64,000 — just under the horizon
    let (n_big, ms_big) = timings[4]; // 85,000 — past it
    let count_ratio = n_big as f64 / n_small as f64;
    let time_ratio = ms_big / ms_small;
    println!("\n  {n_big} / {n_small} = {count_ratio:.2}x the tickets, but {time_ratio:.1}x the time");
    assert!(
        time_ratio > 5.0 * count_ratio,
        "past assign_round's 2^16 horizon (palw_execution_quanta_v1.rs:200-217) cost grows far \
         faster than the ticket count; measured {time_ratio:.1}x time for {count_ratio:.2}x tickets"
    );

    // Fit E*(65536 + E/2)*c on the largest sample and extrapolate to one honest hybrid Final.
    let e_big = (n_big - 65_536) as f64;
    let c_ns = (ms_big * 1e6) / (e_big * (65_536.0 + e_big / 2.0));
    let hybrid = t12_rows().into_iter().find(|r| r.name.starts_with("Qwen3.6")).expect("the hybrid row");
    let hybrid_n = palw_execution_quantum_count_v1(
        hybrid.derived_mac_eq,
        u128::from(PALW_EXECUTION_QUANTUM_V1),
        Hash64::default(),
        Hash64::default(),
    ) as f64;
    let e_hybrid = hybrid_n - 65_536.0;
    let hybrid_s = c_ns * e_hybrid * (65_536.0 + e_hybrid / 2.0) / 1e9;
    println!("\n  fitted c = {c_ns:.1} ns per BTreeSet probe");
    println!("  extrapolated to the hybrid's {hybrid_n:.0} tickets: {hybrid_s:.0} s = {:.1} h", hybrid_s / 3600.0);
    println!("  t12 block interval: 120 s. Even a 20x release-build speedup leaves {:.0} s.", hybrid_s / 20.0);
    assert!(
        hybrid_s / 20.0 > 120.0,
        "one honest hybrid Final's mint must exceed a block interval even allowing a 20x \
         release-build speedup; extrapolated {:.0} s",
        hybrid_s / 20.0
    );
}

// ---------------------------------------------------------------------------------------------
// TEST 5 — the REAL fold: one claim reaching Final writes an unclamped credit, and the span
// rotation mints it into rooted state. This is `apply_palw_transition_v2_with_extras`, the
// function block validation runs, not a model of it.
// ---------------------------------------------------------------------------------------------

const BASE: u64 = 1;
const DEAR: u64 = 2;

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn ctx(word: u64, daa: u64, blue: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: h(word), daa_score: daa, blue_score: blue, subsidy: 0 }
}

fn state_params() -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 10, 10, 20, 500, 1_000, h(BASE), 4, 1_000, 100, 1_000, 0).expect("state params")
}

fn registration(class: u64, share: u16, ceiling: u64) -> Obj {
    Obj::ClassRegistered {
        class_id: h(class),
        artifact_root: h(11),
        slash_value_per_pwu: 1,
        pwu_rule: PalwPwuRuleV2::MaxPerAttempt(ceiling),
        initial_target: u128::MAX / 2,
        share_permille: share,
        activation_daa: 0,
        admission: None,
    }
}

fn bond_object(collateral: u64) -> Obj {
    Obj::BondRegistered {
        bond: bond_key(1),
        pubkey: vec![7; 4],
        operator_pubkey: vec![21; 8],
        collateral,
        payout_payload: Hash64::from_u64_word(0x9A11),
        capable_classes: Default::default(),
        signature: Vec::new(),
    }
}

fn attempt(class: u64, pwu: u64, nonce: u64) -> PalwAttemptEnvelopeV2 {
    PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: h(999),
            challenge: h(nonce ^ 0x00C0_FFEE),
            class_id: h(class),
            executor_bond: bond_key(1).0,
            executor_pubkey: vec![7; 4],
            operator_id: palw_operator_id_v2(&[21u8; 8]),
            artifact_root: h(11),
            trace_root: h(nonce ^ 0x31),
            output_root: h(nonce ^ 0x32),
            pwu,
            trace_manifest_root: h(nonce ^ 0x33),
            trace_chunk_count: 4,
            trace_retention_daa: 999_999,
            execution_root: h(nonce ^ 0x41),
        },
        signature: vec![0; 8],
    }
}

fn lane_extras(execution_quantum: u64) -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 {
        round_lane: Some(PalwExecLaneFoldV1 { schedule_span_daa: 1, execution_quantum, span_open_round: 0 }),
        // t12 arms every fence from DAA 0, the 2026-09-23 credit-unit and ticket-ceiling fix included.
        audit_2026_09_23_active: true,
        ..Default::default()
    }
}

fn step(
    parent: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    c: &PalwBlockContextV2,
    objects: &[Obj],
    att: Option<&PalwAttemptEnvelopeV2>,
    extras: &PalwTransitionExtrasV1,
) -> PalwChainStateV2 {
    let (state, _) = apply_palw_transition_v2_with_extras(parent, p, c, objects, att, false, false, false, false, extras)
        .unwrap_or_else(|e| panic!("the transition at daa {} must apply: {e}", c.daa_score));
    state
}

/// Drive one claim of a `MaxPerAttempt(ceiling)` class from registration to a rooted schedule.
/// Returns (the state holding the schedule, the span it is keyed under, the credit the fold wrote).
fn fold_one_claim_into_a_schedule(claim_pwu: u64, execution_quantum: u64) -> (PalwChainStateV2, u64, u64) {
    let p = state_params();
    let extras = lane_extras(execution_quantum);
    let ceiling = claim_pwu.saturating_mul(2).max(1_000);

    // daa 100 — the floor, the dear class and the bond.
    let s1 = step(
        &PalwChainStateV2::genesis(),
        &p,
        &ctx(1, 100, 1),
        &[registration(BASE, 1_000, 1_000), registration(DEAR, 0, ceiling), bond_object(claim_pwu.saturating_mul(4).max(100_000))],
        None,
        &extras,
    );
    // daa 101 — the attempt.
    let env = attempt(DEAR, claim_pwu, 0xAA);
    let claim_id = attempt_id_v2(&env.attempt);
    let s2 = step(&s1, &p, &ctx(2, 101, 2), &[], Some(&env), &extras);
    // daa 102 / 103 — panel, receipt.
    let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h(90) }];
    let s3 = step(&s2, &p, &ctx(3, 102, 3), &[Obj::PanelBound { claim: claim_id, anchor: h(77), seats }], None, &extras);
    let receipts = vec![PalwSeatReceiptV2 {
        claim: claim_id,
        verdict: PalwReceiptVerdictV2::Valid,
        seat_bond: bond_key(1),
        signed_daa: 0,
        signature: Vec::new(),
    }];
    let s4 = step(&s3, &p, &ctx(4, 103, 4), &[Obj::ReceiptLicensed { claim: claim_id, receipts }], None, &extras);
    // daa 130 — past the challenge window: the sweep matures it and `record_round_final` runs.
    let s5 = step(&s4, &p, &ctx(5, 130, 5), &[], None, &extras);
    assert!(
        matches!(s5.claim(&claim_id).expect("the claim survives").phase, PalwClaimPhaseV2::Final { .. }),
        "the claim must reach Final or record_round_final never runs"
    );
    let (span_of_finals, finals) = s5.round_finals();
    let credit = finals.get(&claim_id).expect("record_round_final wrote a PalwExecFinalV1").credit;
    assert_eq!(span_of_finals, 130, "span_daa = 1, so the finals' span is the DAA score");

    // daa 131 — the boundary takes the snapshot for span 132; this block's attempt anchors the seed.
    let env2 = attempt(DEAR, claim_pwu, 0xBB);
    let s6 = step(&s5, &p, &ctx(6, 131, 6), &[], Some(&env2), &extras);
    assert!(s6.round_pending_snapshot().is_some(), "the boundary must have written a pending snapshot");

    // daa 132 — the snapshot is seeded and the quanta are minted into rooted state.
    let s7 = step(&s6, &p, &ctx(7, 132, 7), &[], None, &extras);
    (s7, 132, credit)
}

#[test]
#[ignore = "PRE-FENCE DEFECT RECORD: below palw_audit_2026_09_23 the fold mints 70,000 tickets from this claim; past it (testnet-12, DAA 0) the mint stops at PALW_EXEC_MAX_QUANTA_PER_SPAN_V1 and repro_06 is the live assertion"]
fn repro_05_the_real_fold_writes_an_unclamped_credit_and_mints_it_into_rooted_state() {
    // A claim whose pwu puts it past assign_round's 2^16 horizon but still mints in seconds here.
    // On t12 this value is not chosen by the attacker at all: it is what the DERIVED basis hands
    // `record_round_final` for an honest class (test 1), and it is 22.6x larger than this.
    const CLAIM_PWU: u64 = 70_000 * PALW_EXECUTION_QUANTUM_V1; // 7,000,000,000

    let t0 = Instant::now();
    let (state, span, credit) = fold_one_claim_into_a_schedule(CLAIM_PWU, PALW_EXECUTION_QUANTUM_V1);
    let fold_ms = t0.elapsed().as_secs_f64() * 1000.0;

    let schedule = state.round_schedule(span).expect("the span boundary wrote a schedule into rooted state");
    let bytes = borsh::to_vec(schedule).expect("the rooted schedule serializes").len();

    println!("\n=== apply_palw_transition_v2_with_extras — the real fold ===");
    println!("  claim.pwu                                  {CLAIM_PWU}");
    println!("  credit record_round_final wrote            {credit}");
    println!("  schedule.finals                            {}", schedule.finals.len());
    println!("  schedule.quanta minted into rooted state   {}", schedule.quanta.len());
    println!("  rooted schedule bytes                      {bytes}");
    println!("  seven-block fold wall time                 {fold_ms:.0} ms");

    assert_eq!(credit, CLAIM_PWU, "the fold wrote the exposure through UNCLAMPED (palw_state_v2.rs:10979)");
    assert_eq!(schedule.quanta.len(), 70_000, "one claim minted 70,000 spend-once tickets into rooted state");
    assert!(bytes > 20_000_000, "70,000 tickets is already >20 MB of rooted schedule; measured {bytes}");

    // The control: with the quantum dormant the very same seven blocks mint nothing at all.
    let (dormant, span_d, credit_d) = fold_one_claim_into_a_schedule(CLAIM_PWU, 0);
    let sched_d = dormant.round_schedule(span_d).expect("the schedule still exists");
    println!("\n  control, execution_quantum = 0:");
    println!("    credit {credit_d}   quanta {}   bytes {}", sched_d.quanta.len(), borsh::to_vec(sched_d).expect("borsh").len());
    assert!(sched_d.quanta.is_empty(), "the dormant lane mints nothing — the mint is what the armed fence buys");
    assert_ne!(state.state_root(), dormant.state_root(), "the quanta are rooted state, not a local view");
}

// ---------------------------------------------------------------------------------------------
// THE MIRROR — the regression test that should pass AFTER the fix. It fails today.
// ---------------------------------------------------------------------------------------------

/// **Ignored on purpose: this is the regression test for the fix, and it FAILS on 077d4c7f.**
///
/// It pins the two properties the finding says are missing, and nothing else:
///
/// 1. **The credit reaches the mint in the unit `PALW_EXECUTION_QUANTUM_V1` is declared in.**
///    The constant's own doc (`palw_execution_quanta_v1.rs:44-47`) says "in the same units
///    `PalwExecFinalV1::credit` is stored in (exposure pwu, capped)", and the whole codebase's
///    converter for that is `palw_exposure_pwu_v3` — derived work renormalised through the floor's
///    own `base_declared / base_canonical`. `record_round_final` calls `palw_exposure_pwu_v2`,
///    which does not renormalise, so this assertion fails by the floor's 2,810x today.
///
/// 2. **`schedule.quanta` is bounded like its two siblings in the same function.**
///    `palw_execution_schedule_snapshot_v1` truncates `domains` at 32 and each domain's `bonds`
///    at 64 (palw_execution_lane_v1.rs:431, :453) precisely because, in its own words, the
///    schedule's "bytes in the state root and a draw's work" must be bounded. `finals` and the
///    `quanta` minted from them carry no such cap, so this assertion fails by 24x today at the
///    generous bound used here.
///
/// Un-ignore it once `record_round_final` normalises the credit (or the mint converts) and the
/// mint or the snapshot caps the ticket count.
#[test]
fn repro_06_mirror_the_mint_is_bounded_and_credited_in_the_quantum_s_own_unit() {
    let rows = t12_rows();
    let floor = rows.iter().find(|r| r.name.starts_with("BASE-0")).expect("the floor row");
    let hybrid = rows.iter().find(|r| r.name.starts_with("Qwen3.6")).expect("the hybrid row");

    // --- property 1: the credit is in exposure pwu, i.e. the floor-normalised unit (U3). ---
    let correct_credit = (hybrid.derived_mac_eq * u128::from(floor.declared_leaves) / floor.derived_mac_eq) as u64;
    // Past `palw_audit_2026_09_23`, `record_round_final` credits through `palw_exposure_pwu_v3` over
    // the floor's exposure basis — the same expression the reservation is written with. Evaluated
    // here over the t12 rows' own basis (the fold resolves the same basis from the base class).
    let basis = kaspa_consensus_core::palw_state_v2::PalwExposureBasisV1 {
        base_declared: floor.declared_leaves,
        base_canonical: floor.derived_mac_eq.min(u64::MAX as u128) as u64,
    };
    let credit_the_fold_writes_past_the_fence = kaspa_consensus_core::palw_state_v2::palw_exposure_pwu_v3(
        &hybrid.class,
        1,
        Some(hybrid.derived_mac_eq.min(u64::MAX as u128) as u64),
        Some(basis),
    );
    let credit_below_the_fence = palw_exposure_pwu_v2(&hybrid.class, 1, Some(hybrid.derived_mac_eq.min(u64::MAX as u128) as u64));
    println!("\n  credit in exposure pwu (U3, correct)        {correct_credit}");
    println!("  credit record_round_final writes, armed    {credit_the_fold_writes_past_the_fence}");
    println!("  credit record_round_final wrote, pre-fence {credit_below_the_fence}  (raw U2, {:.1}x)", credit_below_the_fence as f64 / correct_credit as f64);
    assert_eq!(
        credit_the_fold_writes_past_the_fence, correct_credit,
        "record_round_final must credit the mint in the unit PALW_EXECUTION_QUANTUM_V1 is declared in"
    );

    // --- property 2: a Final cannot mint an unbounded number of rooted tickets. ---
    // A deliberately generous bound: 65,536 is assign_round's own probe horizon, the point past
    // which the mint stops being linear. Nothing in the tree enforces any bound at all.
    const GENEROUS_TICKET_BOUND: usize = 65_536;
    let one = a_final(1, 1, 1, hybrid.derived_mac_eq.min(u64::MAX as u128) as u64);
    let snapshot = palw_execution_schedule_snapshot_v1(7, &[one]);
    let anchor = PalwExecSeedAnchorV1 { span: 6, block: h(6), execution_key: h(0xBEEF) };
    let mut schedule = palw_execution_schedule_seeded_v1(&snapshot, &anchor, 1, h(0xF0F0));
    // The bounded mint the fold runs past the fence.
    kaspa_consensus_core::palw_execution_lane_v1::palw_execution_schedule_assign_quanta_bounded_v1(
        &mut schedule,
        PALW_EXECUTION_QUANTUM_V1,
        0,
        0,
        &std::collections::BTreeSet::new(),
        kaspa_consensus_core::palw_execution_quanta_v1::PALW_EXEC_MAX_QUANTA_PER_SPAN_V1,
    );
    println!("  tickets one honest Qwen3.6@512 Final mints, armed = {}", schedule.quanta.len());
    assert!(
        schedule.quanta.len() <= GENEROUS_TICKET_BOUND,
        "schedule.quanta must be bounded like `domains` and `bonds` are in the same function; \
         one honest Qwen3.6@512 Final minted {}",
        schedule.quanta.len()
    );
    // And through the REAL fold, on the fixture repro_05 uses: 70,000 tickets' worth of credit
    // stops at the horizon.
    let (state, span, _credit) = fold_one_claim_into_a_schedule(70_000 * PALW_EXECUTION_QUANTUM_V1, PALW_EXECUTION_QUANTUM_V1);
    let folded = state.round_schedule(span).expect("the span boundary wrote a schedule into rooted state");
    println!("  tickets the real fold minted from 70,000 quanta of credit, armed = {}", folded.quanta.len());
    assert_eq!(
        folded.quanta.len(),
        kaspa_consensus_core::palw_execution_quanta_v1::PALW_EXEC_MAX_QUANTA_PER_SPAN_V1,
        "the fold's mint stops at the per-span ceiling"
    );
}
