//! **ADR-0145 §8 — the reward properties, stated as things an attacker cannot do.**
//!
//! The 2026-09-19 audit (`docs/adr/STATUS-AUDIT-2026-09-19-llm-mining-reward.md`) found nine of
//! eleven reward invariants violated, and built five counterexamples to prove it. Those
//! counterexamples lived in a scratchpad crate that links this one
//! (`docs/audit/2026-09-19-llm-mining/`). A probe outside the test suite is a probe that rots: it
//! is not run by CI, it is not run by a bisect, and the day somebody changes a cost table it
//! reports nothing. This module brings all five in-tree, and adds the property suite ADR-0145 §8
//! asks for.
//!
//! ## The house rule this module exists to serve
//!
//! > A finding is not closed because a test passes. It is closed when the attack path cannot be
//! > expressed. "The test is green" is how a policy becomes an assumption. — ADR-0145 §8
//!
//! So the tests here come in two kinds and both are load-bearing:
//!
//! * **Counterexample fixtures** pin what the chain does TODAY, with the exact numbers the audit
//!   measured. They pass. They are not approval — they are the baseline a fix has to move, and the
//!   alarm that fires if somebody moves it by accident instead of on purpose.
//! * **Property tests** assert the ADR's invariant. Where the invariant does not hold yet, the test
//!   is `#[ignore]`d with a reason naming EXACTLY which fence must land first. It is never weakened
//!   to pass, because a weakened property test is worse than no property test: it reads like a
//!   guarantee.
//!
//! ## The seam: two functions, and what an integrator repoints
//!
//! Every property here compares two quantities of one claim:
//!
//! * [`fork_weight_of_one_claim_v1`] — what the chain's SECOND fork-choice key buys
//!   (`palw_fork_choice.rs:72-79` reads `safe_weight`, which accumulates `claim.pwu`);
//! * [`executed_mac_eq_of_one_claim_v1`] — what the producer actually spent to buy it.
//!
//! A property is "the first does not move when only a representation moves, while the second is
//! held fixed". **When a fence repoints the accounting at a derived quantity, an integrator
//! repoints these two functions and deletes the `#[ignore]`s — that is the whole migration, and it
//! is why the properties are written against a named seam rather than against
//! `step_leaf_count_capped_v1` directly.** Three such fences exist on branches today, and each
//! `#[ignore]` names the one that closes it.
//!
//! ## Why the difficulty seeding here is the DEFENDER's best case
//!
//! [`fork_weight_of_one_claim_v1`] derives the class target with `palw_work_ticket_target_v1`
//! (ADR-0137): the target that makes one claim cost `W0` MAC-equivalents in expectation. That is
//! the most generous seeding the tree contains — it already normalises for the arithmetic the class
//! really runs, so under it `expected_attempts ≈ W0 / draw`, and a class that executes less has to
//! win more often. **A representation lever that still moves the weight under this seeding is a
//! lever that survives the best difficulty rule in the repo**, which is the only honest way to
//! measure one. The audit's counterexample table uses the same construction, so every number below
//! is comparable to it line for line.

use crate::config::params::palw_rc_shipped_params;
use crate::palw_economic_compute_v1::{
    PALW_ECONOMIC_COST_TABLE_V1, palw_attempt_economic_compute_v1, palw_attempted_compute_per_claim_v1, palw_job_economic_compute_v1,
};
use crate::palw_mode_v2::PalwConsensusMode;
use crate::palw_pwu::{palw_expected_attempts_v1, palw_pwu_v1};
use crate::palw_step::{PalwShapeProfileV3, step_leaf_count_capped_v1, worst_case_step_leaf_count_capped_v1};
use crate::palw_v2::PalwJobContextV2;
use crate::palw_work_target_v1::{palw_work_floor_v1, palw_work_ticket_target_v1};

// ---------------------------------------------------------------------------------------------
// The constants the audit's probe read, and where each one comes from
// ---------------------------------------------------------------------------------------------

/// ADR-0132 Upgrade C's price of compute, sompi per giga-MAC-equivalent. Used only to turn a
/// claim's arithmetic into the PAY column, so the suite can show what moves pay against what moves
/// weight — the audit's central observation being that they are different paths.
const RATE_SOMPI_PER_GIGA: u64 = 900_000_000;

/// The 72 % worker carve of one block's escrow, in sompi. The ceiling `min`ed into every pay figure
/// below, and the reason "unlimited use, scarce reward" holds for pay and not for weight.
const ESCROW_SOMPI: u64 = 320_084_640_000;

/// `palw_prefill_draw` is armed at DAA 4,000 on the shipped preset and testnet-11 passed it long
/// ago, so every execution figure in this module is taken with it TRUE. **This constant is the
/// audit's whole finding in one boolean**: past it an attempt executes `exact_decode_tokens = 1`
/// while the price still counts every declared decode call.
const PREFILL_DRAW_ARMED: bool = true;

/// The ruleset's bisection ladder, read off the shipped params rather than written down — the audit
/// found that three of its four agents worked from a stale flag day because they read fences off
/// the base preset constants instead of at runtime.
fn shipped_ladder() -> u64 {
    let rc = palw_rc_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &rc.palw_consensus_mode else {
        panic!("the shipped release params are a ConsensusV2 bundle");
    };
    bundle.court.max_step_leaf_count()
}

/// The free-prompt lane's shipped ruleset — the quantum divisor and the per-receipt cap.
fn shipped_freeprompt() -> crate::palw_freeprompt_v3::PalwFreePromptParamsV3 {
    let rc = palw_rc_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &rc.palw_consensus_mode else {
        panic!("the shipped release params are a ConsensusV2 bundle");
    };
    bundle.freeprompt.clone()
}

/// `W0` — the work a block's escrow buys at the ADR-0132 rate, in MAC-equivalents.
fn work_floor() -> u128 {
    palw_work_floor_v1(ESCROW_SOMPI, RATE_SOMPI_PER_GIGA)
}

// ---------------------------------------------------------------------------------------------
// The four shipped classes, and the three representation transforms
// ---------------------------------------------------------------------------------------------

/// The dense row testnet-11 runs: Qwen2.5-A16 graph-v5 at `n_ctx` 512. Every counterexample in the
/// audit's families (b) and (d) is a re-declaration of THIS class — one artifact, one kernel set,
/// therefore one certification.
fn dense_512() -> PalwShapeProfileV3 {
    crate::palw_context_ladder::palw_a16_context_row_profile_v5(crate::palw_qwen25_profile::QWEN25_A16_GRAPH_V5_N_CTX)
        .expect("the shipped @512 dense row projects")
}

/// The BASE-0 liveness floor — the class every node must be able to produce.
fn base0_floor() -> PalwShapeProfileV3 {
    crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
        .expect("the shipped floor geometry projects")
}

/// PALW-QWEN36 35B-A3B — the shipped MoE hybrid.
fn hybrid_35b() -> PalwShapeProfileV3 {
    crate::palw_qwen36_profile::qwen36_profile_v2(crate::palw_qwen36_profile::qwen36_geometry_artifact_eps(
        crate::palw_qwen36_profile::QWEN36_35B_A3B,
    ))
    .expect("the shipped hybrid geometry projects")
}

/// Qwen3.8-27B — the widest registered class, and the one F5 says is paid least per unit of the
/// arithmetic it actually runs.
fn dense_27b() -> PalwShapeProfileV3 {
    crate::palw_qwen36_profile::qwen36_profile_v2(crate::palw_qwen36_profile::qwen36_geometry_artifact_eps(
        crate::palw_qwen36_profile::QWEN38_27B,
    ))
    .expect("the shipped 27B geometry projects")
}

fn job(profile: &PalwShapeProfileV3, prefill: u32, decode: u32) -> PalwJobContextV2 {
    crate::palw_base0_profile::rc_job_context(profile, prefill, decode)
}

/// **Transform 1 — re-tiling.** `tile_len` is the commitment's block size: how finely the executor
/// promises to expose its own output for bisection. It changes no arithmetic (asserted at every row
/// of [`the_re_tiled_dense_row`]) and it is free in `[4, 65_536]`.
fn re_tiled(profile: &PalwShapeProfileV3, tile_len: u32) -> PalwShapeProfileV3 {
    let mut out = profile.clone();
    for table in [&mut out.pre_nodes, &mut out.gdn_nodes, &mut out.attn_nodes, &mut out.post_nodes] {
        for node in table.iter_mut() {
            node.tile_len = tile_len;
        }
    }
    out
}

/// **Transform 2 — runtime metadata.** `n_threads` is how many OS threads the executor runs the
/// graph on. It is inside `shape_profile_id` and therefore inside the class id, and it changes
/// nothing a verifier re-executes.
fn with_thread_count(profile: &PalwShapeProfileV3, n_threads: u32) -> PalwShapeProfileV3 {
    let mut out = profile.clone();
    out.n_threads = n_threads;
    out
}

/// **Transform 3 — serialization.** A borsh round trip: the same profile, re-encoded and decoded.
fn round_tripped(profile: &PalwShapeProfileV3) -> PalwShapeProfileV3 {
    let bytes = borsh::to_vec(profile).expect("a shape profile serializes");
    borsh::from_slice(&bytes).expect("a shape profile round-trips")
}

// ---------------------------------------------------------------------------------------------
// THE SEAM — the two quantities every property compares
// ---------------------------------------------------------------------------------------------

/// **What one accepted claim of this class contributes to fork choice, as the chain computes it
/// today.**
///
/// `claim.pwu = expected_attempts(class_target) × pwu_per_inference`
/// (`palw_pwu::palw_pwu_v1`), where `pwu_per_inference` is the STEP-LEAF count of the canonical job
/// the REGISTRANT declares (`palw_class_admission_v2.rs:2152` binds declared == counted, and never
/// binds the graph to arithmetic). `safe_weight` accumulates it and `palw_fork_choice.rs:72-79`
/// reads `safe_weight` as its second key.
///
/// **This is the function an integrator repoints.** Past `palw_canonical_work` (branch
/// `worktree-wf_8f1fb519-50c-1`) the accounting reads a quantity derived from the graph and the
/// execution facts instead of the declared leaf count; repointing this one function is what turns
/// every `#[ignore]`d property below into a live assertion.
fn fork_weight_of_one_claim_v1(profile: &PalwShapeProfileV3, canonical: (u32, u32)) -> u64 {
    let context = job(profile, canonical.0, canonical.1);
    let declared_leaves =
        step_leaf_count_capped_v1(profile, &context, shipped_ladder()).expect("the fixture's canonical job is under the ladder");
    palw_pwu_v1(class_target_v1(profile, canonical), declared_leaves)
}

/// **What that claim actually cost its producer**, in ADR-0131 MAC-equivalents: `expected_attempts`
/// draws, each an execution of `palw_attempt_v2::palw_attempt_job_v1` — the job the chain really
/// runs, which past `palw_prefill_draw` is `(declared_prefill, 1)` whatever decode budget the
/// registrant declared.
fn executed_mac_eq_of_one_claim_v1(profile: &PalwShapeProfileV3, canonical: (u32, u32)) -> u128 {
    let draw = one_draw_mac_eq_v1(profile, canonical);
    palw_attempted_compute_per_claim_v1(palw_expected_attempts_v1(class_target_v1(profile, canonical)), draw)
}

/// One draw's arithmetic — the unit `executed_mac_eq_of_one_claim_v1` multiplies up, and the one
/// number in this module that is neither declared nor declarable.
fn one_draw_mac_eq_v1(profile: &PalwShapeProfileV3, canonical: (u32, u32)) -> u128 {
    palw_attempt_economic_compute_v1(
        profile,
        &job(profile, canonical.0, canonical.1),
        PREFILL_DRAW_ARMED,
        &PALW_ECONOMIC_COST_TABLE_V1,
    )
    .expect("the fixture's canonical job walks")
}

/// The class target under ADR-0137's work target: the target at which one claim costs `W0`.
fn class_target_v1(profile: &PalwShapeProfileV3, canonical: (u32, u32)) -> u128 {
    palw_work_ticket_target_v1(one_draw_mac_eq_v1(profile, canonical), work_floor())
}

/// What the block is paid, past ADR-0132 Upgrade C: the arithmetic it really ran, priced at the
/// rate, and `min`ed into the escrow. The second half of "weight and pay are on different paths".
fn pay_sompi_of_one_claim_v1(profile: &PalwShapeProfileV3, canonical: (u32, u32)) -> u64 {
    let attempted = executed_mac_eq_of_one_claim_v1(profile, canonical);
    ((attempted * RATE_SOMPI_PER_GIGA as u128) / 1_000_000_000u128).min(ESCROW_SOMPI as u128) as u64
}

/// **Weight per unit of arithmetic in ONE DRAW**, scaled by 1e9 so the comparison is integer.
///
/// This is the audit's published `weight/MAC-eq` column, and the one the 24,572x headline is
/// computed from.
fn weight_per_giga_drawn_v1(profile: &PalwShapeProfileV3, canonical: (u32, u32)) -> u128 {
    let weight = fork_weight_of_one_claim_v1(profile, canonical) as u128;
    weight.saturating_mul(1_000_000_000) / one_draw_mac_eq_v1(profile, canonical).max(1)
}

/// **Weight per unit of arithmetic the producer really spent to win the BLOCK**, scaled by 1e9.
///
/// The same weight over `expected_attempts × draw` instead of over one draw. The two ratios answer
/// different questions and the report should carry both: per draw the shipped row and the
/// admissible extreme are 24,572x apart, because the extreme's draw is 1/54 the arithmetic; per
/// CLAIM they are 457x apart, because the extreme has to win 230 times as often to produce a block
/// at all. **457x is what a producer actually buys for its money**, and it is the number that
/// bounds the attack; 24,572x is the number that says the accounting has no relation to the work.
fn weight_per_giga_attempted_v1(profile: &PalwShapeProfileV3, canonical: (u32, u32)) -> u128 {
    let weight = fork_weight_of_one_claim_v1(profile, canonical) as u128;
    weight.saturating_mul(1_000_000_000) / executed_mac_eq_of_one_claim_v1(profile, canonical).max(1)
}

/// Declared step leaves per giga-MAC-equivalent of one executed draw. F5's basis figure.
fn declared_leaves_per_giga_executed_v1(profile: &PalwShapeProfileV3, canonical: (u32, u32)) -> u128 {
    let leaves = step_leaf_count_capped_v1(profile, &job(profile, canonical.0, canonical.1), shipped_ladder())
        .expect("under the ladder") as u128;
    leaves.saturating_mul(1_000_000_000) / one_draw_mac_eq_v1(profile, canonical).max(1)
}

// =============================================================================================
// PART 1 — the audit's five counterexamples, as fixtures
// =============================================================================================
//
// Each is a PASSING test pinning today's numbers, followed by an `#[ignore]`d assertion of what the
// fence must make true. The pinned numbers are the ones in
// `docs/audit/2026-09-19-llm-mining/counterexample-table.txt` and `close-output.txt`; pinning them
// here is what stops the scratchpad crate from being the only place they exist.

/// **Counterexample 1 — the decode declaration.** Audit family (b)/(d), F1's larger lever.
///
/// One graph, one artifact, one kernel set, one certification, four declarations of the canonical
/// `(P, D)`. Every row is admissible. The executed arithmetic of the `(63, ·)` rows is IDENTICAL —
/// past `palw_prefill_draw` the draw is `(63, 1)` in all three — and the fork-choice weight is not.
#[test]
fn the_decode_declaration() {
    let dense = dense_512();
    let (canonical_p, canonical_d) = crate::palw_qwen25_profile::qwen25_a16_graph_v5_canonical_v1();
    assert_eq!((canonical_p, canonical_d), (63, 2), "the shipped dense row's canonical job; every row below re-declares it");

    // The three `(63, D)` rows execute the same draw, because the draw drops the decode budget.
    let shipped_draw = one_draw_mac_eq_v1(&dense, (63, 2));
    assert_eq!(shipped_draw, 83_102_171_136, "one draw of the shipped dense row, in MAC-equivalents");
    for decode in [2u32, 128, 370] {
        assert_eq!(
            one_draw_mac_eq_v1(&dense, (63, decode)),
            shipped_draw,
            "declaring {decode} decode calls does not make the draw execute any of them — \
             palw_attempt_job_v1 sets exact_decode_tokens = 1 past palw_prefill_draw"
        );
    }

    // And the weight is not the same at all.
    assert_eq!(fork_weight_of_one_claim_v1(&dense, (63, 2)), 26_522_176);
    assert_eq!(fork_weight_of_one_claim_v1(&dense, (63, 128)), 88_022_272);
    assert_eq!(fork_weight_of_one_claim_v1(&dense, (63, 370)), 206_141_504);
    // 7.77x, the audit's headline "7.8x the weight for identical arithmetic".
    assert_eq!(
        206_141_504u128 * 100 / 26_522_176u128,
        777,
        "declaring 370 decode calls instead of 2 buys 7.77x the fork-choice weight for byte-identical execution"
    );

    // Pay does not move with it, which is why one of the two went unexamined: they are different
    // paths. ADR-0132 Upgrade C prices the arithmetic; nothing prices the weight.
    assert_eq!(pay_sompi_of_one_claim_v1(&dense, (63, 2)), pay_sompi_of_one_claim_v1(&dense, (63, 370)));

    // The admissible extreme: (1, 432) runs 1/54 of the arithmetic and buys 457x the weight.
    assert_eq!(one_draw_mac_eq_v1(&dense, (1, 432)), 1_546_037_392);
    assert_eq!(fork_weight_of_one_claim_v1(&dense, (1, 432)), 12_124_304_640);
    assert_eq!(
        weight_per_giga_drawn_v1(&dense, (1, 432)) / weight_per_giga_drawn_v1(&dense, (63, 2)),
        24_572,
        "24,572x the weight per unit of arithmetic in one DRAW — the audit's published column, \
         recomputed here from the repo's own functions rather than transcribed"
    );
    assert_eq!(
        weight_per_giga_attempted_v1(&dense, (1, 432)) / weight_per_giga_attempted_v1(&dense, (63, 2)),
        427,
        "and 427x per unit of arithmetic the producer spent to win the BLOCK — `close-output.txt` \
         C5's 427.3x. **Three different numbers are all correct and the report has to say which it \
         means**: 457x is the raw weight of one block against another's; 427x is that weight per \
         unit of what its producer really spent, which is what the attack BUYS; 24,572x is the \
         weight per unit of one draw, which is what says the accounting has no relation to the work"
    );

    // Admissibility, checked the way the chain checks it, so nobody has to take the word for it:
    // both rows are under the class's own declared worst case and inside its context.
    let worst =
        worst_case_step_leaf_count_capped_v1(&dense, shipped_ladder()).expect("the dense row's worst case is under the ladder");
    assert_eq!(worst, 52_778_128);
    for (p, d) in [(63u32, 2u32), (63, 370), (1, 432)] {
        let leaves = step_leaf_count_capped_v1(&dense, &job(&dense, p, d), shipped_ladder()).unwrap();
        assert!(leaves <= worst, "({p},{d}) is inside the class's declared worst case");
        assert!(p as u64 + d.max(1) as u64 - 1 <= dense.n_ctx as u64, "({p},{d}) fits the class's context");
    }
}

/// The neutralisation counterexample 1 must reach.
#[test]
#[ignore = "needs fence `palw_canonical_work` (branch worktree-wf_8f1fb519-50c-1, commit b79b2331) \
            AND `fork_weight_of_one_claim_v1` repointed at palw_canonical_work_v1::palw_canonical_draw_work_v1"]
fn the_decode_declaration_is_neutralised() {
    let dense = dense_512();
    let shipped = fork_weight_of_one_claim_v1(&dense, (63, 2));
    for decode in [2u32, 128, 370] {
        assert_eq!(
            fork_weight_of_one_claim_v1(&dense, (63, decode)),
            shipped,
            "a declared decode budget the draw never runs must not be in the weight"
        );
    }
}

/// **Counterexample 2 — the re-tiled dense row.** Audit family (a), F1's `tile_len` lever.
///
/// `tile_len` is the commitment's block size. It changes no arithmetic, no kernel and no answer.
/// Across the tilings the chain ADMITS it moves the fork-choice weight by 134.9x.
///
/// **This pins a bound the audit's own prose understates.** The report calls `tile_len` a lever
/// "free in [4, 65_536]"; `close-output.txt` C1 then refutes the naive version, because admission
/// stores `worst_case_step_leaf_count_capped_v1(profile, ladder)` and that call ERRORS above the
/// ladder, so tile 16 and finer are refused outright. The surviving lever is the span between the
/// coarsest and the finest tiling that both clear the ladder — tile 65,536 and tile 24 — and it is
/// 134.9x, not the 101x an implementer's summary reports.
#[test]
fn the_re_tiled_dense_row() {
    let dense = dense_512();
    let canonical = (63u32, 2u32);
    let ladder = shipped_ladder();
    let arithmetic = palw_job_economic_compute_v1(&dense, &job(&dense, 63, 2), &PALW_ECONOMIC_COST_TABLE_V1).unwrap();
    assert_eq!(arithmetic, 84_653_733_376);
    let kernels = crate::palw_class_admission_v2::reachable_kernels_v1(&dense);

    // (tile_len, declared leaves, fork weight). Admissible rows only.
    let admissible: [(u32, u64, u64); 8] = [
        (65_536, 43_146, 172_584),
        (4_096, 64_720, 258_880),
        (512, 280_542, 1_122_168),
        (128, 1_089_910, 4_359_640),
        (64, 2_179_820, 8_719_280),
        (48, 2_913_596, 11_654_384),
        (32, 4_359_640, 17_438_560),
        (24, 5_821_814, 23_287_256),
    ];
    for (tile, leaves, weight) in admissible {
        let profile = re_tiled(&dense, tile);
        // Nothing about the execution moved. Asserted, not assumed — this is what makes the row a
        // REPRESENTATION and not a different class of work.
        assert_eq!(
            palw_job_economic_compute_v1(&profile, &job(&profile, 63, 2), &PALW_ECONOMIC_COST_TABLE_V1).unwrap(),
            arithmetic,
            "re-tiling at {tile} must not move ADR-0131's arithmetic"
        );
        assert_eq!(crate::palw_class_admission_v2::reachable_kernels_v1(&profile), kernels, "nor the reachable kernel set");
        assert_eq!(one_draw_mac_eq_v1(&profile, canonical), 83_102_171_136, "nor what one draw executes");
        // The declared leaf count and therefore the weight moved by two orders of magnitude.
        assert_eq!(step_leaf_count_capped_v1(&profile, &job(&profile, 63, 2), ladder).unwrap(), leaves, "tile {tile}");
        assert_eq!(fork_weight_of_one_claim_v1(&profile, canonical), weight, "tile {tile}");
        assert!(
            worst_case_step_leaf_count_capped_v1(&profile, ladder).is_ok(),
            "tile {tile} is admissible: its declared worst case clears the ladder"
        );
    }
    assert_eq!(
        (23_287_256u128 * 10) / 172_584u128,
        1349,
        "134.9x the fork-choice weight across the tilings the chain admits, for one model, one \
         artifact, one kernel set and byte-identical arithmetic"
    );

    // The bound is real, and it is the ladder that sets it — not the accounting. Tile 16 is
    // refused at admission, by exactly 43,520 leaves.
    let too_fine = re_tiled(&dense, 16);
    assert_eq!(
        worst_case_step_leaf_count_capped_v1(&too_fine, ladder),
        Err(crate::palw_step::PalwStepError::TooManyLeaves { got: 67_152_384, max: 67_108_864 }),
        "a tiling finer than 24 is refused because its declared worst case exceeds the ladder — \
         which is a bound on the COURT's walk, not on the price, and stops being one the moment the \
         ladder is raised"
    );
}

/// The neutralisation counterexample 2 must reach.
#[test]
#[ignore = "needs fence `palw_canonical_work` (branch worktree-wf_8f1fb519-50c-1, commit b79b2331) \
            AND `fork_weight_of_one_claim_v1` repointed at palw_canonical_work_v1::palw_canonical_draw_work_v1"]
fn the_re_tiled_dense_row_is_neutralised() {
    let dense = dense_512();
    let shipped = fork_weight_of_one_claim_v1(&dense, (63, 2));
    for tile in [65_536u32, 4_096, 512, 128, 64, 48, 32, 24] {
        assert_eq!(
            fork_weight_of_one_claim_v1(&re_tiled(&dense, tile), (63, 2)),
            shipped,
            "the commitment's block size is not work and must not be paid as work"
        );
    }
}

/// **Counterexample 3 — the better model is paid less.** Audit F5, live, no registration needed.
///
/// Leaves count ACTIVATIONS; the cost table counts WEIGHTS. So the measure and the cost diverge in
/// model width, and they diverge monotonically: across the four registered classes the declared
/// work per unit of executed arithmetic spans 6.8x, ordered by size, with the widest model at the
/// bottom.
#[test]
fn the_better_model_is_paid_less() {
    let rows: [(&str, PalwShapeProfileV3, (u32, u32), u128); 4] = [
        ("BASE-0 floor", base0_floor(), crate::palw_base0_profile::PALW_RC_BASE0_CANONICAL, 355_900),
        ("Qwen3.6-35B-A3B", hybrid_35b(), crate::palw_qwen36_profile::QWEN36_RC_CANONICAL, 148_730),
        ("Qwen2.5-A16 @512", dense_512(), (63, 2), 79_787),
        ("Qwen3.8-27B", dense_27b(), crate::palw_qwen36_profile::QWEN36_RC_CANONICAL, 52_051),
    ];
    let mut previous: Option<u128> = None;
    for (name, profile, canonical, expected) in &rows {
        let per_giga = declared_leaves_per_giga_executed_v1(profile, *canonical);
        assert_eq!(per_giga, *expected, "{name}: declared step leaves per giga-MAC-equivalent executed");
        if let Some(previous) = previous {
            assert!(per_giga < previous, "{name}: the basis falls monotonically as the model gets wider");
        }
        previous = Some(per_giga);
    }
    assert_eq!(355_900u128 * 10 / 52_051u128, 68, "a 6.8x spread, and the widest model is at the wrong end of it");

    // The same fact in the units that decide fork choice: per unit of the arithmetic one DRAW
    // executes, the floor buys 18,311 times the weight the dense row does. (The audit's index table
    // prints this as 1,831,182.5 against 100, which is the same ratio scaled by a hundred.)
    let floor_ratio = weight_per_giga_drawn_v1(&base0_floor(), crate::palw_base0_profile::PALW_RC_BASE0_CANONICAL);
    let dense_ratio = weight_per_giga_drawn_v1(&dense_512(), (63, 2));
    assert_eq!(floor_ratio / dense_ratio, 18_311, "got {floor_ratio} vs {dense_ratio}");
}

/// The neutralisation counterexample 3 must reach.
///
/// **This one is not closed by any of the three fences**, and the `#[ignore]` says which ADR owes
/// it: a basis that prices the four classes within a stated bound needs the coefficients ADR-0146
/// declines to invent, and the residency experiment ADR-0146 §6 requires has not been run. The
/// bound asserted here — 2x, chosen because it is the loosest number that still refuses a 6.8x
/// spread — is a PLACEHOLDER and is marked as one, so that whoever arms a basis has to decide the
/// real number rather than inherit this one.
#[test]
#[ignore = "needs an ARBITRAGE BOUND, which no branch has: ADR-0146 R7's re-runnable adversarial \
            search and the §6 one-host residency measurement. palw_canonical_work makes the basis \
            derived; it does not make it neutral, and provisional_scalar_v1 weights the byte \
            dimensions at ZERO, so it prices width no better than leaves do"]
fn the_better_model_is_paid_less_is_neutralised() {
    let rows: [(PalwShapeProfileV3, (u32, u32)); 4] = [
        (base0_floor(), crate::palw_base0_profile::PALW_RC_BASE0_CANONICAL),
        (hybrid_35b(), crate::palw_qwen36_profile::QWEN36_RC_CANONICAL),
        (dense_512(), (63, 2)),
        (dense_27b(), crate::palw_qwen36_profile::QWEN36_RC_CANONICAL),
    ];
    let ratios: Vec<u128> = rows.iter().map(|(p, c)| weight_per_giga_drawn_v1(p, *c)).collect();
    let (lo, hi) = (ratios.iter().min().copied().unwrap().max(1), ratios.iter().max().copied().unwrap());
    assert!(hi / lo <= 2, "PLACEHOLDER BOUND — the real one is ADR-0146's to measure; got {hi} / {lo}");
}

/// **Counterexample 4 — the padded prefix.** Audit C4, F2's cache channel.
///
/// The free-prompt lane's `work_leaves` is the executor's own field and the acceptance walk never
/// recomputes it, so the honest number is already unchecked. This fixture pins the channel that
/// survives even if it WERE recomputed honestly: a producer holding the prefix's KV cache pays the
/// same 3.09 G MAC-equivalents at every prompt length, while the credited work — and the pwu, and
/// the exposure — rises 62x with the padding.
#[test]
fn the_padded_prefix() {
    let dense = dense_512();
    let ladder = shipped_ladder();
    let freeprompt = shipped_freeprompt();
    let canonical_leaves = step_leaf_count_capped_v1(&dense, &job(&dense, 63, 2), ladder).unwrap();
    assert_eq!(canonical_leaves, 6_630_544, "the class's canonical job, which is the quantum's denominator");

    // What a producer who already holds the prefix's KV cache spends to extend it by one position.
    // Taken as the (1, 2) job — one new prefill position and one decode call — which is the
    // marginal work the cache leaves, and is CONSTANT in the padding.
    let marginal_with_cache = palw_job_economic_compute_v1(&dense, &job(&dense, 1, 2), &PALW_ECONOMIC_COST_TABLE_V1).unwrap();
    assert_eq!(marginal_with_cache, 3_092_162_480);

    // (prompt tokens, honestly counted leaves, quanta, claim.pwu, honest arithmetic)
    let rows: [(u32, u64, u32, u64, u128); 6] = [
        (8, 965_104, 1, 828_818, 12_283_845_456),
        (63, 6_630_544, 8, 6_630_544, 84_653_733_376),
        (128, 13_326_064, 16, 13_261_088, 170_523_797_136),
        (256, 26_511_088, 31, 25_693_358, 340_704_989_840),
        (400, 41_344_240, 49, 40_612_082, 533_876_270_096),
        (500, 51_645_040, 62, 51_386_716, 669_092_883_696),
    ];
    for (prompt, leaves, quanta, pwu, honest) in rows {
        assert_eq!(step_leaf_count_capped_v1(&dense, &job(&dense, prompt, 2), ladder).unwrap(), leaves, "prompt {prompt}");
        assert_eq!(
            palw_job_economic_compute_v1(&dense, &job(&dense, prompt, 2), &PALW_ECONOMIC_COST_TABLE_V1).unwrap(),
            honest,
            "prompt {prompt}: what an UNCACHED producer really spends"
        );
        assert_eq!(
            freeprompt.derive_quanta_and_pwu(leaves, canonical_leaves),
            Some((quanta, pwu)),
            "prompt {prompt}: the transition's own derivation of quanta and pwu"
        );
    }

    // The channel, in one number: 62x the pwu for a producer whose marginal cost did not move.
    assert_eq!(51_386_716u64 / 828_818u64, 62, "62x the claim pwu between an 8-token prompt and a 500-token one");
    // And the cap that bounds it: 64 quanta per receipt, so the ceiling is 8x the class's own
    // canonical job. The channel is therefore a way for a SMALL job to reach the cap, not an
    // unbounded pump — which is why it is a 62x and not an arbitrary multiple.
    assert_eq!(freeprompt.max_quanta_per_receipt(), 64);
    assert_eq!(
        freeprompt.derive_quanta_and_pwu(u64::MAX, canonical_leaves),
        Some((64, 64 * (canonical_leaves / 8))),
        "the per-receipt cap holds against any work_leaves at all, which is the one thing that keeps \
         the self-report from being unbounded"
    );
}

/// The neutralisation counterexample 4 must reach.
#[test]
#[ignore = "needs fence `palw_fp_derived_work` (branch worktree-wf_8f1fb519-50c-3, commits 981d22af \
            and 501e644b) AND a seam for the derived credit — the prefix the chain already bought \
            for this (class, bond) is read off its own rooted record, which this module has no \
            state to hold"]
fn the_padded_prefix_is_neutralised() {
    let dense = dense_512();
    let ladder = shipped_ladder();
    let freeprompt = shipped_freeprompt();
    let canonical_leaves = step_leaf_count_capped_v1(&dense, &job(&dense, 63, 2), ladder).unwrap();
    // The claim: a 500-token prompt whose first 499 tokens the chain has already paid this bond for
    // must be credited like the extension it is, not like a 500-token prefill. Expressed here as
    // the leaves of the job that was actually NEW.
    let extension_only = step_leaf_count_capped_v1(&dense, &job(&dense, 1, 2), ladder).unwrap();
    let padded = step_leaf_count_capped_v1(&dense, &job(&dense, 500, 2), ladder).unwrap();
    assert_eq!(
        freeprompt.derive_quanta_and_pwu(padded, canonical_leaves),
        freeprompt.derive_quanta_and_pwu(extension_only, canonical_leaves),
        "a prefix the chain already bought is not bought again"
    );
}

/// **Counterexample 5 — the class admitted at a ladder its own legal jobs exceed.** Audit F4.
///
/// `worst_case_step_leaf_count_capped_v1` enumerates `n_ctx - 1` prefill positions and ONE decode
/// call. A real job pays a logits term at EVERY decode call, so `(1, n_ctx)` is deeper than the
/// declared worst case by 18.4 % on this geometry. A class whose declared worst case clears the
/// ladder while its deepest legal job does not is **accepted and unprosecutable**: the court's
/// refutation walker is capped at the ladder, so no dispute over such a claim can be opened at all.
///
/// The shipped @512 row is safe. One registration at `n_ctx` 576 is not, and the whole
/// 576..=640 band is reachable.
#[test]
fn the_class_admitted_at_a_ladder_its_jobs_exceed() {
    let ladder = shipped_ladder();
    assert_eq!(ladder, 67_108_864, "2^26, the shipped ruleset's bisection ladder");

    // (n_ctx, declared worst case, deepest legal job). The deepest legal job is (1, n_ctx): one
    // prompt token and a full context of decode calls, each paying its own logits pass.
    let rows: [(u32, u64, u64); 4] =
        [(512, 52_778_128, 62_476_288), (576, 59_370_640, 70_285_824), (608, 62_666_896, 74_190_592), (640, 65_963_152, 78_095_360)];
    for (n_ctx, declared_worst, deepest_legal) in rows {
        let profile = crate::palw_context_ladder::palw_a16_context_row_profile_v5(n_ctx).expect("the row projects");
        assert_eq!(worst_case_step_leaf_count_capped_v1(&profile, ladder).unwrap(), declared_worst, "n_ctx {n_ctx}");
        assert_eq!(
            step_leaf_count_capped_v1(&profile, &job(&profile, 1, n_ctx), u64::MAX).unwrap(),
            deepest_legal,
            "n_ctx {n_ctx}: the deepest job the class's own context admits"
        );
        // The gap is a property of the enumeration, not of the geometry: 18.4 % at every width.
        assert_eq!((deepest_legal as u128 * 1000) / declared_worst as u128, 1183, "n_ctx {n_ctx}: an 18.4 % gap");

        let admitted = declared_worst <= ladder;
        let prosecutable = deepest_legal <= ladder;
        assert!(admitted, "n_ctx {n_ctx} is ADMITTED — its declared worst case is under the ladder");
        if n_ctx == 512 {
            assert!(prosecutable, "the SHIPPED row is safe: even its deepest legal job is walkable");
        } else {
            assert!(
                !prosecutable,
                "n_ctx {n_ctx} holds legal jobs the court cannot walk: admitted, and a claim on one \
                 of them can never be disputed"
            );
        }
        // And the job is legal by the chain's own footprint rule, so this is not a hypothetical.
        assert!(1u64 + n_ctx as u64 - 1 <= profile.n_ctx as u64, "n_ctx {n_ctx}: (1, n_ctx) fits the class's own context");
        assert_eq!(
            step_leaf_count_capped_v1(&profile, &job(&profile, 1, n_ctx), ladder).is_err(),
            !prosecutable,
            "n_ctx {n_ctx}: the walker refuses exactly the jobs that are over the ladder, which is \
             the same cap the court's refutation walk runs under"
        );
    }
}

/// The neutralisation counterexample 5 must reach.
///
/// **No branch closes this.** All three implementers say so in their own words. The fix is not a
/// fence: it is `worst_case_step_leaf_count_capped_v1` enumerating the class's deepest legal job rather
/// than one decode call, which changes a number every registered class was admitted against.
#[test]
#[ignore = "needs worst_case_step_leaf_count_capped_v1 to enumerate the deepest LEGAL job (a logits \
            pass at every decode call), not one decode call. No branch touches it; all three \
            implementers list F4 under does-not-close"]
fn the_over_ladder_class_is_refused_at_admission() {
    let ladder = shipped_ladder();
    for n_ctx in [512u32, 576, 608, 640] {
        let profile = crate::palw_context_ladder::palw_a16_context_row_profile_v5(n_ctx).expect("the row projects");
        let declared_worst = worst_case_step_leaf_count_capped_v1(&profile, ladder).unwrap();
        let deepest_legal = step_leaf_count_capped_v1(&profile, &job(&profile, 1, n_ctx), u64::MAX).unwrap();
        assert!(
            deepest_legal <= declared_worst,
            "invariant (vii): a class's declared worst case must bound EVERY legal job. \
             n_ctx {n_ctx}: {deepest_legal} > {declared_worst}"
        );
    }
}

/// **The fix for counterexample 5: a worst case that is one** —
/// [`crate::palw_step::worst_case_step_leaf_count_deepest_job_capped_v1`].
///
/// Invariant (vii), stated as the audit states it: *a class's declared worst case must bound EVERY
/// legal job*. The predicate under test is the new closed form; the thing it must bound is
/// `step_leaf_count_capped_v1` at every `(P, D)` the class's own footprint rule admits
/// (`palw_step_leg`'s `JobExceedsClassContext`: `P + exact_decode_tokens - 1 ≤ n_ctx`).
///
/// Three properties, and the third is the one that closes F4:
///
/// * it EQUALS the deepest legal job's own count, so it is a bound that is attained and not a
///   margin somebody guessed;
/// * it DOMINATES every job in a sweep over the admissible `(P, D)` lattice, on all four shipped
///   profiles and on the whole 512..=640 A16 band;
/// * the 576..=640 band — admitted today — is REFUSED against the shipped `2^26` ladder, while the
///   shipped @512 row still clears it, so the fix refuses exactly the classes whose legal jobs the
///   court cannot walk and no shipped class is lost.
#[test]
fn the_deepest_legal_job_is_the_worst_case() {
    use crate::palw_step::worst_case_step_leaf_count_deepest_job_capped_v1 as deepest;
    let ladder = shipped_ladder();
    assert_eq!(ladder, 67_108_864, "2^26, the shipped ruleset's bisection ladder");

    // (1) and (2), on every shipped profile.
    for (name, profile) in
        [("base0", base0_floor()), ("dense@512", dense_512()), ("hybrid35B", hybrid_35b()), ("dense27B", dense_27b())]
    {
        let n_ctx = profile.n_ctx;
        let bound = deepest(&profile, u64::MAX).expect("the closed form evaluates");
        assert_eq!(
            bound,
            step_leaf_count_capped_v1(&profile, &job(&profile, 1, n_ctx), u64::MAX).unwrap(),
            "{name}: the bound is the deepest legal job's own count, attained"
        );
        // …and it is never below the OLD function, which is the half the old one got right.
        assert!(bound >= worst_case_step_leaf_count_capped_v1(&profile, u64::MAX).unwrap(), "{name}: a worst case never shrinks");
        // The sweep: every `(P, D)` the footprint rule admits, at the corners and across the band.
        for p in [0u32, 1, 2, 7, 63, n_ctx / 4, n_ctx / 2, n_ctx - 1, n_ctx] {
            for d in [1u32, 2, 8, 64, n_ctx / 2, n_ctx] {
                // `JobExceedsClassContext`: `P + exact_decode_tokens − 1 ≤ n_ctx`.
                if p as u64 + d.max(1) as u64 - 1 > n_ctx as u64 {
                    continue;
                }
                let counted = step_leaf_count_capped_v1(&profile, &job(&profile, p, d), u64::MAX).unwrap();
                assert!(counted <= bound, "{name}: job ({p}, {d}) counts {counted} leaves, above the worst case {bound}");
            }
        }
    }

    // (3) The band the audit measured, priced against the shipped ladder.
    // (n_ctx, the OLD declared worst, the deepest legal job).
    let rows: [(u32, u64, u64); 4] =
        [(512, 52_778_128, 62_476_288), (576, 59_370_640, 70_285_824), (608, 62_666_896, 74_190_592), (640, 65_963_152, 78_095_360)];
    for (n_ctx, old_worst, deepest_legal) in rows {
        let profile = crate::palw_context_ladder::palw_a16_context_row_profile_v5(n_ctx).expect("the row projects");
        assert_eq!(worst_case_step_leaf_count_capped_v1(&profile, ladder).unwrap(), old_worst, "n_ctx {n_ctx}: the old number");
        assert_eq!(deepest(&profile, u64::MAX).unwrap(), deepest_legal, "n_ctx {n_ctx}: the new number is the deepest legal job");
        if n_ctx == 512 {
            assert_eq!(deepest(&profile, ladder), Ok(deepest_legal), "the SHIPPED row still clears the ladder under the fix");
        } else {
            assert_eq!(
                deepest(&profile, ladder),
                Err(crate::palw_step::PalwStepError::TooManyLeaves { got: deepest_legal, max: ladder }),
                "n_ctx {n_ctx} is refused: its legal jobs are above the ladder the court walks at"
            );
        }
    }
}

// =============================================================================================
// PART 2 — ADR-0145 §8's properties
// =============================================================================================

/// **Representation invariance, the half that already holds: serialization.**
///
/// A profile re-encoded and decoded is the same class with the same price. Cheap, and worth a test
/// because `shape_profile_id` is canonical borsh over the whole profile — a field whose encoding
/// was not canonical would make one model two classes at two prices, silently, and the round trip
/// is the only place that shows.
#[test]
fn serialization_is_not_a_representation_choice() {
    for profile in [dense_512(), base0_floor(), hybrid_35b(), dense_27b()] {
        let back = round_tripped(&profile);
        assert_eq!(back.shape_profile_id(), profile.shape_profile_id());
        assert_eq!(back, profile, "a round trip is the identity on the profile itself, not merely on its id");
    }
    let dense = dense_512();
    assert_eq!(fork_weight_of_one_claim_v1(&round_tripped(&dense), (63, 2)), fork_weight_of_one_claim_v1(&dense, (63, 2)));
}

/// **Representation invariance, the half the graph validator already closes: node order.**
///
/// ADR-0145 §8 lists node order among the representations that must not move the price. It does not
/// move it, and the reason is worth pinning: a reordered graph does not VALIDATE. Nodes name their
/// inputs, `validate_shape` requires a definition before a use, and a permutation breaks that
/// before any price is computed.
///
/// So node order is closed by the validator, not by the accounting — and this test is what fires if
/// a future change relaxes the ordering rule, because at that moment `shape_profile_id` (canonical
/// borsh over the node vectors, order included) would start minting one model as many classes at
/// many prices, exactly as `tile_len` does today.
#[test]
fn a_reordered_graph_is_refused_before_it_can_be_priced() {
    let dense = dense_512();
    assert!(dense.validate_shape().is_ok(), "the shipped row validates");
    let mut reversed_post = dense.clone();
    reversed_post.post_nodes.reverse();
    assert!(reversed_post.validate_shape().is_err(), "a reordered post table is refused: a node may not be used before it is defined");
    let mut reversed_pre = dense.clone();
    reversed_pre.pre_nodes.reverse();
    assert!(reversed_pre.validate_shape().is_err(), "and a reordered pre table likewise");
}

/// **Registrant independence — the property, and the measurement that refutes it today.**
///
/// ADR-0145 §8: "No registrant-writable field increases canonical work, reward or weight." Three
/// registrant-writable fields are swept here. Two of them move the weight by two orders of
/// magnitude. The third — `n_threads`, the executor's own thread count — does not move the weight,
/// but it DOES move the class id, which is the same defect wearing the other face: one model
/// registered twice is two classes, two share grants and two prices (audit C3).
#[test]
fn registrant_writable_fields_move_the_weight_today() {
    let dense = dense_512();
    let shipped = fork_weight_of_one_claim_v1(&dense, (63, 2));

    // 1. The canonical decode budget.
    assert!(fork_weight_of_one_claim_v1(&dense, (63, 370)) > shipped * 7);
    // 2. The commitment's tile length. Note the SHAPE of this lever, because it is not the shape
    //    the audit's prose suggests: re-tiling the SHIPPED row cannot raise its own weight (the
    //    shipped tiling is already near the fine end, and finer than 24 is refused at admission).
    //    What the registrant chooses is where in a 134.9x band its class sits, and two registrants
    //    of ONE model at two tilings are paid 134.9x apart for identical arithmetic.
    let coarsest = fork_weight_of_one_claim_v1(&re_tiled(&dense, 65_536), (63, 2));
    let finest_admissible = fork_weight_of_one_claim_v1(&re_tiled(&dense, 24), (63, 2));
    assert!(finest_admissible / coarsest > 100, "{finest_admissible} vs {coarsest}");
    assert!(coarsest < shipped && finest_admissible < shipped, "and the shipped row sits inside that band, not at its top");
    // 3. Runtime metadata: the weight does not move, but the CLASS does.
    let relabelled = with_thread_count(&dense, dense.n_threads.wrapping_add(1).max(1));
    assert_eq!(fork_weight_of_one_claim_v1(&relabelled, (63, 2)), shipped, "a thread count is not work, and is not priced as work");
    assert_ne!(
        relabelled.shape_profile_id(),
        dense.shape_profile_id(),
        "but it IS a different class id, so one model can be registered twice for two share grants \
         and certified by the same kernel subset both times"
    );
    assert_eq!(
        crate::palw_class_admission_v2::reachable_kernels_v1(&relabelled),
        crate::palw_class_admission_v2::reachable_kernels_v1(&dense),
        "and the second registration certifies on the first's kernel set"
    );
}

/// The property itself.
#[test]
#[ignore = "needs fence `palw_canonical_work` (branch worktree-wf_8f1fb519-50c-1, commit b79b2331) \
            for the weight half, and `PalwCanonicalClassDescriptorV1::canonical_class_id_v1` to be \
            what a SHARE GRANT is keyed by for the class-id half — which no branch does: the \
            descriptor is economic only, and palw_admission_independence leaves registration keyed \
            on shape_profile_id"]
fn no_registrant_writable_field_increases_the_weight() {
    let dense = dense_512();
    let shipped = fork_weight_of_one_claim_v1(&dense, (63, 2));
    for decode in [2u32, 128, 370, 432] {
        let canonical = if decode == 432 { (1, 432) } else { (63, decode) };
        assert!(fork_weight_of_one_claim_v1(&dense, canonical) <= shipped, "declared decode budget {decode}");
    }
    for tile in [65_536u32, 4_096, 512, 128, 64, 48, 32, 24] {
        assert_eq!(fork_weight_of_one_claim_v1(&re_tiled(&dense, tile), (63, 2)), shipped, "tile {tile}");
    }
    assert_eq!(with_thread_count(&dense, 8).shape_profile_id(), dense.shape_profile_id(), "a thread count does not mint a class");
}

/// **Executor independence.** ADR-0145 §8: "Tamper with every declared workload value; consensus
/// derives the same answer or refuses the claim."
///
/// The free-prompt lane's `work_leaves` is the executor's own field. This test tampers with it and
/// measures what the chain's own derivation does with the lie: it scales, up to the per-receipt
/// cap. The repo's own test asserts the opposite invariant — `palw_fp_objects_v3.rs:885` multiplies
/// `work_leaves` by ten and asserts the acceptance walk still credits the carrier — and that test
/// is the audit's evidence for F2, not a bug in the audit.
#[test]
fn the_free_prompt_lanes_declared_work_is_the_executors_own_number() {
    let dense = dense_512();
    let freeprompt = shipped_freeprompt();
    let canonical_leaves = step_leaf_count_capped_v1(&dense, &job(&dense, 63, 2), shipped_ladder()).unwrap();
    let honest = step_leaf_count_capped_v1(&dense, &job(&dense, 8, 2), shipped_ladder()).unwrap();
    assert_eq!(honest, 965_104);

    let (_, honest_pwu) = freeprompt.derive_quanta_and_pwu(honest, canonical_leaves).unwrap();
    let (_, tenfold_pwu) = freeprompt.derive_quanta_and_pwu(honest * 10, canonical_leaves).unwrap();
    assert_eq!(honest_pwu, 828_818);
    assert_eq!(
        tenfold_pwu, 9_116_998,
        "a tenfold lie is an elevenfold pwu: the derivation reads the lie, floors it into quanta \
         (9,651,040 / 828,818 = 11) and multiplies the quantum back out. It does not ask the graph \
         what the run would have counted, because on this path it holds no graph to ask"
    );
    assert!(tenfold_pwu > honest_pwu * 10, "and the floor loses the liar less than one quantum of the lie");
}

/// The property itself.
#[test]
#[ignore = "needs fence `palw_fp_derived_work` (branch worktree-wf_8f1fb519-50c-3, commits 981d22af \
            and 501e644b): past it the extraction walk recomputes the leaves from the class's \
            published graph and SKIPS a disagreeing carrier, and the transition refuses the same \
            commitment by name (FreePromptWorkLeavesMismatch). This test needs the walk's entry \
            point, which is not a pure function of (profile, leaves)"]
fn tampering_with_the_declared_work_changes_no_price() {
    let dense = dense_512();
    let freeprompt = shipped_freeprompt();
    let canonical_leaves = step_leaf_count_capped_v1(&dense, &job(&dense, 63, 2), shipped_ladder()).unwrap();
    let honest = step_leaf_count_capped_v1(&dense, &job(&dense, 8, 2), shipped_ladder()).unwrap();
    for multiple in [1u64, 2, 10, 1_000] {
        assert_eq!(
            freeprompt.derive_quanta_and_pwu(honest * multiple, canonical_leaves),
            freeprompt.derive_quanta_and_pwu(honest, canonical_leaves),
            "a declared work_leaves of {multiple}x the truth prices as the truth, or is refused"
        );
    }
}

/// **Efficiency preservation, the half that holds.** ADR-0145 §8: "The same canonical work on
/// faster or cheaper hardware earns no less."
///
/// Within a class this is structural and it is worth pinning as such: nothing on the pricing path
/// takes a timing input. `n_threads` is the only host-speed knob a profile carries, and moving it
/// moves neither the leaf count, nor the arithmetic, nor the weight, nor the pay. A future change
/// that started reading `PALW_VERIFICATION_REFERENCE_V1.mac_eq_per_ms` or a registry `replay_cost`
/// into the price would break this test, which is exactly the change `palw_pwu.rs:44-50` forbids in
/// prose and nothing enforces.
#[test]
fn a_faster_host_earns_no_less() {
    let dense = dense_512();
    let baseline = (
        fork_weight_of_one_claim_v1(&dense, (63, 2)),
        pay_sompi_of_one_claim_v1(&dense, (63, 2)),
        one_draw_mac_eq_v1(&dense, (63, 2)),
    );
    for threads in [1u32, 2, 8, 64] {
        let fast = with_thread_count(&dense, threads);
        assert_eq!(
            (
                fork_weight_of_one_claim_v1(&fast, (63, 2)),
                pay_sompi_of_one_claim_v1(&fast, (63, 2)),
                one_draw_mac_eq_v1(&fast, (63, 2))
            ),
            baseline,
            "running the same graph on {threads} threads is the same work at the same price"
        );
    }
}

/// **Cache correctness.** ADR-0145 §8: "Work already computed cannot be paid twice; a cached prefix
/// cannot be presented as new."
///
/// Today there is no place on any object for the chain to learn that a prefix was reused, so the
/// property is not merely violated — it is unrepresentable in the other direction. This test pins
/// the consequence in the price: the credited work is a function of the prompt LENGTH alone, and a
/// producer's actual marginal cost is not in it anywhere.
#[test]
fn a_reused_prefix_is_paid_for_as_if_it_were_new() {
    let dense = dense_512();
    let freeprompt = shipped_freeprompt();
    let ladder = shipped_ladder();
    let canonical_leaves = step_leaf_count_capped_v1(&dense, &job(&dense, 63, 2), ladder).unwrap();

    // Two runs with the SAME marginal cost to a producer holding the prefix: one new position and
    // one decode call. The chain prices them 62x apart, because it prices the prompt.
    let short = step_leaf_count_capped_v1(&dense, &job(&dense, 8, 2), ladder).unwrap();
    let padded = step_leaf_count_capped_v1(&dense, &job(&dense, 500, 2), ladder).unwrap();
    let (_, short_pwu) = freeprompt.derive_quanta_and_pwu(short, canonical_leaves).unwrap();
    let (_, padded_pwu) = freeprompt.derive_quanta_and_pwu(padded, canonical_leaves).unwrap();
    assert_eq!(padded_pwu / short_pwu, 62);
}

/// The property itself.
#[test]
#[ignore = "needs fence `palw_fp_derived_work` (branch worktree-wf_8f1fb519-50c-3) AND the rooted \
            per-(class, bond) prefix record its PalwFpExecutionModeV1::PrefixReused is derived \
            from. The KV half stays open on that branch too: a producer that computed a prefix \
            locally and never claimed it is still credited the whole prefill, and closing that \
            needs the prefix-STATE commitment ADR-0145 §6 names, which moves the object's wire"]
fn a_reused_prefix_is_not_paid_for_twice() {
    let dense = dense_512();
    let freeprompt = shipped_freeprompt();
    let ladder = shipped_ladder();
    let canonical_leaves = step_leaf_count_capped_v1(&dense, &job(&dense, 63, 2), ladder).unwrap();
    let extension = step_leaf_count_capped_v1(&dense, &job(&dense, 1, 2), ladder).unwrap();
    let padded = step_leaf_count_capped_v1(&dense, &job(&dense, 500, 2), ladder).unwrap();
    assert_eq!(
        freeprompt.derive_quanta_and_pwu(padded, canonical_leaves),
        freeprompt.derive_quanta_and_pwu(extension, canonical_leaves),
        "the 499 tokens the chain already bought are not bought again"
    );
}

/// **Unlimited use, scarce reward.** ADR-0145 §8: "Local inference is unbounded; eligible work never
/// exceeds the protocol budget."
///
/// This holds for PAY and does not hold for WEIGHT, and the asymmetry is the audit's structural
/// point: `min(.., escrow)` bounds what a block is paid, and nothing anywhere bounds what a block
/// weighs. The heaviest admissible row below buys 457x the shipped row's weight while its pay stays
/// inside 7 % of it.
#[test]
fn pay_is_bounded_and_weight_is_not() {
    let dense = dense_512();
    for canonical in [(63u32, 2u32), (63, 370), (1, 256), (1, 432)] {
        assert!(pay_sompi_of_one_claim_v1(&dense, canonical) <= ESCROW_SOMPI, "pay is capped at the block's escrow, always");
    }
    let shipped_pay = pay_sompi_of_one_claim_v1(&dense, (63, 2));
    let extreme_pay = pay_sompi_of_one_claim_v1(&dense, (1, 432));
    assert!(extreme_pay * 100 / shipped_pay <= 107, "the extreme is paid within 7 % of the shipped row");

    let shipped_weight = fork_weight_of_one_claim_v1(&dense, (63, 2)) as u128;
    let extreme_weight = fork_weight_of_one_claim_v1(&dense, (1, 432)) as u128;
    assert_eq!(extreme_weight / shipped_weight, 457, "and weighs 457x as much, against no ceiling at all");

    // There is no ceiling to find: the weight is `expected_attempts × declared_leaves`, and
    // `palw_pwu_v1` saturates only at `u64::MAX`. A ceiling that only exists at saturation is not a
    // ceiling; it is an overflow guard.
    assert_eq!(palw_pwu_v1(0, u64::MAX), u64::MAX, "the tightest target and the largest declaration saturate rather than refuse");
}

/// The property itself.
#[test]
#[ignore = "needs a per-class or per-epoch CEILING on eligible weight. ADR-0145 §7 asks for one \
            ('Probation bounds VOLUME, not price'), and palw_admission_independence implements the \
            lifecycle's admission_permille but nothing caps the weight a single class contributes. \
            No branch closes this"]
fn eligible_weight_never_exceeds_the_protocol_budget() {
    let dense = dense_512();
    let shipped = fork_weight_of_one_claim_v1(&dense, (63, 2));
    for canonical in [(63u32, 370u32), (1, 256), (1, 432)] {
        assert!(
            fork_weight_of_one_claim_v1(&dense, canonical) <= shipped * 2,
            "one class's claim may not buy an unbounded multiple of another's"
        );
    }
}

/// **Fork determinism.** ADR-0145 §8: "Archival, pruned, IBD, pruning-proof join and post-reorg
/// nodes derive the same work, eligibility, weight and reward."
///
/// The part this module can hold is the part that matters most and is cheapest to lose: every
/// function on the pricing path is a pure function of `(profile, job)`, so two nodes that agree on
/// those two cannot disagree on the price. Pinned three ways — repeated calls, a serialization round
/// trip, and the cap argument, which must not change an answer that is under it.
#[test]
fn the_price_is_a_pure_function_of_the_class_and_the_job() {
    let dense = dense_512();
    let ladder = shipped_ladder();
    let first = (fork_weight_of_one_claim_v1(&dense, (63, 2)), executed_mac_eq_of_one_claim_v1(&dense, (63, 2)));
    for _ in 0..4 {
        assert_eq!((fork_weight_of_one_claim_v1(&dense, (63, 2)), executed_mac_eq_of_one_claim_v1(&dense, (63, 2))), first);
    }
    let back = round_tripped(&dense);
    assert_eq!((fork_weight_of_one_claim_v1(&back, (63, 2)), executed_mac_eq_of_one_claim_v1(&back, (63, 2))), first);

    // A node that walks with a wider cap must get the same answer for a job that fits under the
    // narrower one — otherwise an archival node and a pruned node with different ladders in force
    // would price one claim two ways.
    let context = job(&dense, 63, 2);
    let at_ladder = step_leaf_count_capped_v1(&dense, &context, ladder).unwrap();
    assert_eq!(step_leaf_count_capped_v1(&dense, &context, u64::MAX).unwrap(), at_ladder);
    assert_eq!(step_leaf_count_capped_v1(&dense, &context, at_ladder).unwrap(), at_ladder, "the cap is inclusive at its own boundary");
    assert!(step_leaf_count_capped_v1(&dense, &context, at_ladder - 1).is_err(), "and refuses one leaf below it");
}

/// **Self-admission.** ADR-0145 §8: "A class whose owner controls 100 % of the seats declaring
/// capability for it cannot be admitted."
///
/// It can. The predicate that decides whether a bond may judge a class is
/// `palw_bond_may_judge_class_v2(bond, class_id) == bond.capable_classes.contains(class_id)` — the
/// bond's own declaration and nothing else. `palw_capability_bound` is dormant on every shipped
/// preset, so that IS the rule in force, and `palw_panel_eligible_bonds_v2` adds only the executor
/// exclusion on top of it: the executing bond, its operator id and its pubkey.
///
/// **The executor exclusion is why the obvious independence check would be a fence that never
/// fires.** When a registrant produces its own class's claims, its producing bond, operator and key
/// are already out of the draw, so every remaining seat passes any test built from that triple. The
/// registrant's OTHER bonds are strangers to it. Seven of them are built here, each with its own
/// bond key and its own operator id — which is the real Sybil price,
/// `min_collateral_sompi` and an ML-DSA-87 key apiece — and every one is eligible.
#[test]
fn a_registrants_own_seats_may_judge_its_own_class() {
    use crate::palw_state_v2::{PalwBondStateV2, PalwBondStatusV2, palw_bond_may_judge_class_v2, palw_operator_id_v2};
    use kaspa_hashes::Hash64;

    let class_id = Hash64::from_bytes([7u8; 64]);
    let one_payee = Hash64::from_bytes([9u8; 64]);
    let registrant_seats: Vec<PalwBondStateV2> = (0u8..7)
        .map(|i| PalwBondStateV2 {
            pubkey: vec![i; 32],
            operator_id: palw_operator_id_v2(&[i; 32]),
            collateral: 1_000_000_000,
            slashed: 0,
            status: PalwBondStatusV2::Active,
            registered_daa: 0,
            // One payout address. Under any beneficial-ownership notion these are one party; the
            // chain holds no such notion, which is the gap the fix has to price rather than prove.
            payout_payload: one_payee,
            capable_classes: std::collections::BTreeSet::from([class_id]),
        })
        .collect();

    for (i, seat) in registrant_seats.iter().enumerate() {
        assert!(
            palw_bond_may_judge_class_v2(seat, &class_id),
            "seat {i} of the registrant's own fleet is eligible to judge its class"
        );
    }
    // Distinct operator ids, so the panel's dedup does not collapse them into one seat.
    let operators: std::collections::BTreeSet<_> = registrant_seats.iter().map(|b| b.operator_id).collect();
    assert_eq!(operators.len(), 7, "seven distinct operator identities, which is the whole cost of the attack");
    // And one payee, which is the thing nothing looks at.
    let payees: std::collections::BTreeSet<_> = registrant_seats.iter().map(|b| b.payout_payload).collect();
    assert_eq!(payees.len(), 1);
}

/// The property itself.
#[test]
#[ignore = "needs fence `palw_admission_independence` (branch worktree-wf_8f1fb519-50c-2, commit \
            b5fc03c2): past it a PanelBound of a class with a registrant_bond must seat at least \
            one bond the registrant does not hold, the licensing quorum must NAME such a seat, and \
            a class does not leave Candidate without one. This test needs the fold's predicate, \
            which takes a PalwChainStateV2 and a class record this module does not build"]
fn a_class_whose_owner_holds_every_capable_seat_cannot_be_admitted() {
    // The shape the fence must make true, written against the only predicate that exists today so
    // that it compiles: judging a class must depend on something other than the judge's own
    // declaration. When the fence lands this is re-pointed at the fold's independence predicate,
    // which takes the class's `registrant_bond` as its second input.
    unimplemented!(
        "the independence predicate takes (state, class record, seat) and has no pure form; \
         palw_admission_independence's own test \
         `a_class_whose_registrant_holds_every_ready_seat_is_not_admissible_past_the_fence` is the \
         live version of this property"
    );
}

/// **Registered is not eligible.** ADR-0145 §7.
///
/// Half of this is already in the tree and is worth pinning before anybody adds a state for it: the
/// registry's own lifecycle enum ALREADY says a `Registered` class admits no claims and takes no
/// admission. What is missing is not the state — it is that nothing reads it. `palw_model_registry`
/// is `None` on every shipped preset since the 2026-09-19 disarm, and `check_class_admits_claim`
/// consults a lifecycle row only when `work_target_active`, which is dormant with it.
///
/// **This is a note for the integrator**, and it is why it is a test rather than a comment: an
/// implementer added a new `Candidate` variant ahead of `Registered` in the lifecycle. The variant
/// carries the independence condition for LEAVING it, which is real; the "admits no claims, bears
/// no weight" half of what it is for is what `Registered` already does, and appending a borsh
/// discriminant is not free.
#[test]
fn the_registry_already_says_a_registered_class_admits_nothing() {
    use crate::palw_model_registry_v1::PalwModelLifecycleV1;
    assert!(!PalwModelLifecycleV1::Registered.admits_claims());
    assert_eq!(PalwModelLifecycleV1::Registered.admission_permille(), 0);
    assert!(!PalwModelLifecycleV1::Prefetching.admits_claims());
    assert_eq!(PalwModelLifecycleV1::Prefetching.admission_permille(), 0);
    assert!(PalwModelLifecycleV1::Probation { probes_passed: 0 }.admits_claims());
    assert_eq!(PalwModelLifecycleV1::Probation { probes_passed: 0 }.admission_permille(), 50);
    assert_eq!(PalwModelLifecycleV1::Active.admission_permille(), 1_000);
}

// =============================================================================================
// PART 3 — the integration trap
// =============================================================================================

/// **`palw_canonical_work` may not precede `palw_model_registry`, and the reason is not tidiness.**
///
/// This test was written on a branch that also carried the 2026-09-19 disarm (`805056ac`), which
/// put `palw_model_registry` back to `None` everywhere; it asserted the registry was dormant and
/// called the ordering guard a trap, on the ground that the derivation "reads no registry row to
/// price a claim". **The disarm was not integrated** — the operator's design fixes the lifecycle
/// itself as the brake — and that ground turns out to be false, which reverses the conclusion.
///
/// `PalwChainStateV2::palw_canonical_per_draw_v1` sources the derived work from
/// `model_lifecycles[class].work.economic_ccu_per_claim`, and `model_lifecycles` is written by
/// `write_model_lifecycle` from eight sites that all sit behind `model_registry_fold()`. Below the
/// registry's height the table is EMPTY. So a build that armed `palw_canonical_work` first would
/// derive `None` for every class, and `None` means *fall back to the declared basis* — the fence
/// would announce that weight had stopped being a registrant's number while changing nothing at
/// all. That is worse than leaving it dormant, because it is the same failure wearing a fix.
///
/// The ordering guard in `validate_palw_v2` is therefore load-bearing, and this test pins the three
/// facts it rests on: the registry IS armed on the shipped params, the table is empty below it, and
/// the guard refuses the inverted order.
///
/// What stays true from the original note: **F1 is live today.** `claim.pwu` needs no fence, and
/// testnet-11 already carries post-genesis classes. The fix cannot land before DAA
/// [`crate::config::params::PALW_RC_PALW_UPGRADE_FENCE_DAA`], and the window until then is exposure
/// the schedule cannot remove — not a reason to arm the derivation over an empty table.
#[test]
fn canonical_work_cannot_precede_the_registry_that_fills_the_table_it_derives_from() {
    use crate::config::params::{ForkActivation, PALW_RC_PALW_UPGRADE_FENCE_DAA};
    let rc = palw_rc_shipped_params();
    let registry = rc.palw_model_registry.expect("the shipped params arm the model registry — the lifecycle is the brake");
    assert_eq!(registry.daa_score(), PALW_RC_PALW_UPGRADE_FENCE_DAA, "and it arms at the upgrade fence");

    // The guard refuses the inverted order and accepts the level and later ones.
    let mut below = rc.clone();
    below.palw_canonical_work = Some(ForkActivation::new(PALW_RC_PALW_UPGRADE_FENCE_DAA - 1));
    let refusal = below.validate_palw_v2().expect_err("a canonical-work fence below the registry must be refused");
    assert!(format!("{refusal:?}").contains("palw_canonical_work"), "{refusal:?}");
    // With its bundle: ADR-0145's three fences arm at one height or not at all.
    for height in [PALW_RC_PALW_UPGRADE_FENCE_DAA, PALW_RC_PALW_UPGRADE_FENCE_DAA + 1_000] {
        let mut ok = rc.clone();
        ok.palw_canonical_work = Some(ForkActivation::new(height));
        ok.palw_admission_independence = Some(ForkActivation::new(height));
        ok.palw_fp_derived_work = Some(ForkActivation::new(height));
        ok.validate_palw_v2().unwrap_or_else(|e| panic!("at or past the registry it assembles: {e:?}"));
    }

    // And the substantive reason, demonstrated rather than described: the derivation reads a
    // registry row, so over the table as it stands BEFORE the registry has run — empty — it
    // answers `None` for every class, at any height, including heights past the fence. `None` is
    // "keep the declared basis", so an earlier arming would change nothing while announcing that
    // it had. Genesis is exactly that state: classes exist, lifecycle rows do not.
    let genesis = crate::palw_state_v2::PalwChainStateV2::genesis();
    assert!(genesis.model_lifecycles_iter().next().is_none(), "genesis carries no lifecycle row");
    for (class_id, _) in genesis.classes_iter() {
        for daa in [PALW_RC_PALW_UPGRADE_FENCE_DAA, PALW_RC_PALW_UPGRADE_FENCE_DAA + 1_000_000] {
            assert_eq!(
                genesis.palw_canonical_per_draw_v1(class_id, daa, Some(PALW_RC_PALW_UPGRADE_FENCE_DAA)),
                None,
                "with no registry row the derivation has nothing to answer with, so the declared basis survives the fence"
            );
        }
    }
}

/// **The identity an independence rule would be built on is a declaration, not a possession.**
///
/// `palw_panel_eligible_bonds_v2` excludes the executor's own bond, `operator_id` and pubkey, and
/// any rule that asks "is this seat the registrant's" has to be built from the same materials. Of
/// those materials only the bond outpoint and its pubkey cost anything: `operator_id` is derived
/// from an operator pubkey NOBODY EVER VERIFIES A SIGNATURE UNDER — `palw_operator_id_unique`'s own
/// doc says so, and that fence is `None` on every preset — and `payout_payload` is a 64-byte owner
/// payload the registrant picks.
///
/// So the marginal price of looking like a second party is one more bond at `min_collateral_sompi`
/// paid to a different address, which is exactly what the executor exclusion already charges. A
/// fence that adds the payee to the exclusion triple adds an address to the shopping list; it
/// becomes a real cost only when `palw_operator_id_unique` arms beside it, because that is the
/// fence that makes a second identity cost a second ML-DSA-87 key and prove possession of it.
///
/// This test pins the precondition so the two are armed together or not at all.
#[test]
fn the_identity_a_panel_dedups_on_is_a_declaration_today() {
    let rc = palw_rc_shipped_params();
    assert_eq!(
        rc.palw_operator_id_unique, None,
        "operator_id is self-declared and unverified on every shipped network; an admission-\
         independence rule that prices a Sybil at 'a second ML-DSA-87 key and its own collateral' \
         is quoting a fence that is not in force"
    );
    for preset in [
        crate::config::params::MAINNET_PARAMS,
        crate::config::params::TESTNET_PARAMS,
        crate::config::params::SIMNET_PARAMS,
        crate::config::params::DEVNET_PARAMS,
    ] {
        assert_eq!(preset.palw_operator_id_unique, None);
    }
}

/// **The collateral a claim reserves is denominated in the unit the accounting reads, and moving
/// the unit moves the collateral.**
///
/// `palw_exposure_pwu_v1` returns `pwu_per_inference` — STEP LEAVES — and the fold multiplies it by
/// the class's `slash_value_per_pwu` (5 sompi on the shipped genesis registry) to get what one
/// claim reserves against its bond. A fence that repoints this at a DERIVED quantity in ADR-0131
/// MAC-equivalents changes the unit by the ratio pinned here, and `slash_value_per_pwu` is frozen
/// at registration.
///
/// The consequence is not an accounting nicety, and `palw_exposure_pwu_v1`'s own doc — forty lines
/// above the place a fence would change — is the warning: a per-claim reservation that outgrows the
/// bond makes a producer hit `ExposureCeilingExceeded` **for succeeding**, and on the floor class a
/// refused attempt is `StatusDisqualifiedFromChain`, so no block, so the DAA does not advance, so
/// nothing that depends on a clock recovers. "The chain stops for getting used."
///
/// This test measures the ratio so that whoever arms a derived basis has to rescale
/// `slash_value_per_pwu` by it, or say in writing why not.
#[test]
fn a_derived_basis_changes_the_unit_the_collateral_is_denominated_in() {
    const SLASH_VALUE_PER_PWU: u64 = 5; // palw_genesis_v2.rs:497, the shipped registry's value

    // (class, canonical job, declared leaves, one draw's MAC-equivalents, the ratio between them)
    let rows: [(&str, PalwShapeProfileV3, (u32, u32), u64, u128, u128); 2] = [
        ("BASE-0 floor", base0_floor(), crate::palw_base0_profile::PALW_RC_BASE0_CANONICAL, 7_708, 21_657_728, 2_809),
        ("Qwen2.5-A16 @512", dense_512(), (63, 2), 6_630_544, 83_102_171_136, 12_533),
    ];
    for (name, profile, canonical, declared_leaves, drawn_mac_eq, ratio) in &rows {
        assert_eq!(
            step_leaf_count_capped_v1(profile, &job(profile, canonical.0, canonical.1), shipped_ladder()).unwrap(),
            *declared_leaves,
            "{name}: what palw_exposure_pwu_v1 returns today"
        );
        assert_eq!(one_draw_mac_eq_v1(profile, *canonical), *drawn_mac_eq, "{name}: what a derived basis would return instead");
        assert_eq!(drawn_mac_eq / *declared_leaves as u128, *ratio, "{name}");
    }

    // In sompi, against a 10,000 MSK bond: the dense row's claim goes from 0.33 MSK reserved to
    // 4,155 MSK — 41.5 % of a whole bond for ONE claim, where testnet-11 measured ~500 concurrent
    // seats per bond. The floor's goes from 0.00039 MSK to 1.08 MSK.
    assert_eq!(6_630_544u128 * SLASH_VALUE_PER_PWU as u128, 33_152_720, "0.33 MSK reserved per dense claim today");
    assert_eq!(83_102_171_136u128 * SLASH_VALUE_PER_PWU as u128, 415_510_855_680, "4,155 MSK per dense claim on a MAC-eq basis");
}

/// **Invariant (vi): the protocol's difficulty is not seeded from a registrant's declaration.**
///
/// `attempt_target_seed_v1(share, pwu)` decides a class's target, hence `expected_attempts`, hence
/// `claim.pwu`, hence fork-choice weight. Both of its live call sites read
/// `palw_max_exposure_pwu_of_rule_v1` of the class's own rule — the registrant's declared number —
/// with no canonical fallback at all, unlike the work price, which had one. That is F1's remaining
/// path into consensus, and the 2026-09-19 re-audit found it unfixed after the accounting branch
/// landed: the branch repointed the price and the reservation and left the difficulty.
///
/// Both now run `class_seed_pwu`. This reads the fold rather than a model of it, because the
/// defect was a MISSING call, and no behavioural fixture can fail for a call that is not there.
#[test]
fn the_difficulty_seed_reads_no_registrant_declaration() {
    let source = include_str!("palw_state_v2.rs");
    let fold = &source[..source.find("\n#[cfg(test)]").expect("the tests follow the fold")];

    let sites: Vec<&str> = fold.match_indices("attempt_target_seed_v1(").map(|(at, _)| &fold[at.saturating_sub(220)..at]).collect();
    assert!(sites.len() >= 2, "both live seeding sites are still here ({})", sites.len());
    for (n, before) in sites.iter().enumerate() {
        assert!(
            !before.contains("palw_max_exposure_pwu_of_rule_v1"),
            "seeding site {n} still takes its pwu from the class's declared rule"
        );
    }
    assert_eq!(fold.matches("self.class_seed_pwu(").count() + fold.matches("builder.class_seed_pwu(").count(), 2, "and both take it from one expression");
}

/// **Where a pwu is normalised and where it must not be.**
///
/// `palw_exposure_pwu_v3` converts derived work into the unit the collateral was posted in. Applying
/// it everywhere would be as wrong as applying it nowhere: a pwu that meets an ABSOLUTE constant
/// needs the unit (the reservation meets `slash_value_per_pwu`; the difficulty seed meets
/// `PALW_ATTEMPT_TARGET_UNIT_SHARE_PWU_V1`, past which the target saturates and `expected_attempts`
/// pins at one), while a pwu that meets ANOTHER PWU is already a ratio and the unit divides out —
/// normalising one side of `escrow × own pwu / unit` would break the cancellation it exists to
/// preserve. This pins the rule so a later branch cannot tidy it into uniformity.
#[test]
fn the_price_unit_stays_raw_because_it_is_a_denominator() {
    let source = include_str!("palw_state_v2.rs");
    let fold = &source[..source.find("\n#[cfg(test)]").expect("the tests follow the fold")];
    let at = fold.find("fn work_price_unit_at(").expect("the work price is still here");
    let body = &fold[at..at + 600];
    assert!(body.contains("self.canonical_per_draw(id, accepted_daa)"), "the price unit reads the derived work RAW");
    assert!(
        !body.contains("exposure_basis") && !body.contains("palw_exposure_pwu_v3"),
        "and it must not be normalised: it is a denominator, and the numerator is not"
    );
}
