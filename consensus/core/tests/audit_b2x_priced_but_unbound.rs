//! **AUDIT B2 (re-run) — the priced fields, and what binds them.**
//!
//! Read-only audit artefact. Nothing here is a fixture any shipped code reads.

use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
use kaspa_consensus_core::palw_economic_compute_v1::{PALW_ECONOMIC_COST_TABLE_V1, palw_attempt_economic_compute_v1};
use kaspa_consensus_core::palw_fp_devnet_v3::{PalwCollateralRowV1, palw_exposure_unit_pwu_v1, palw_v2_collateral_for_class_set_v1};
use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7, qwen25_a16_held_canonical_v1};
use kaspa_consensus_core::palw_qwen36_profile::{PalwQwen36GeometryV1, QWEN36_35B_A3B, qwen36_geometry_artifact_eps, qwen36_held_canonical_v1, qwen36_profile_v7};
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;

const HYBRID_N_CTX: u32 = 512;
const DENSE_N_CTX: u32 = 2_097_152;
/// `PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI` (config/premine.rs).
const POSTED_SOMPI: u64 = 51_642_979_663_480;
/// `escrow_for_a_genesis_claim_v1` on the t12 card: 370_468_345 / 1000 * 720.
const GENESIS_ESCROW_SOMPI: u64 = 266_736_960;
const SLASH: u128 = 5;

fn floor() -> PalwShapeProfileV3 {
    base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("floor")
}
fn hybrid() -> PalwShapeProfileV3 {
    qwen36_profile_v7(qwen36_geometry_artifact_eps(PalwQwen36GeometryV1 { n_ctx: HYBRID_N_CTX, ..QWEN36_35B_A3B })).expect("hybrid")
}
fn dense() -> PalwShapeProfileV3 {
    qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: DENSE_N_CTX, ..QWEN25_1_5B }).expect("dense")
}
fn draw(p: &PalwShapeProfileV3, pf: u32, de: u32) -> u128 {
    palw_attempt_economic_compute_v1(p, &rc_job_context(p, pf, de), true, &PALW_ECONOMIC_COST_TABLE_V1).expect("prices")
}
fn msk(sompi: u128) -> f64 {
    sompi as f64 / 1e8
}

/// **The floor's canonical job is a `pub const` in the binary. It is the DENOMINATOR of every
/// class's collateral reservation.**
#[test]
fn b2x_the_exposure_basis_denominator_is_a_build_constant() {
    let f = floor();
    let d = dense();
    let hy = hybrid();
    let dense_draw = draw(&d, qwen25_a16_held_canonical_v1(DENSE_N_CTX).0, qwen25_a16_held_canonical_v1(DENSE_N_CTX).1);
    let hy_draw = draw(&hy, qwen36_held_canonical_v1(HYBRID_N_CTX).0, qwen36_held_canonical_v1(HYBRID_N_CTX).1);

    println!("\nPALW_RC_BASE0_CANONICAL (palw_base0_profile.rs:905) = {PALW_RC_BASE0_CANONICAL:?} — a build constant.");
    println!("It appears in NO chain object. Genesis pins the floor's DECLARED LEAVES (7,708), not its MAC-eq.\n");
    println!("{:<14} {:>16} {:>22} {:>22} {:>18}", "floor (P,D)", "floor MAC-eq", "dense exposure pwu", "dense reserved sompi", "seat collateral");
    for cand in [PALW_RC_BASE0_CANONICAL, (8, 2), (11, 2), (4, 4), (1, 2)] {
        let floor_draw = draw(&f, cand.0, cand.1);
        let unit = palw_exposure_unit_pwu_v1(dense_draw, 7_708, floor_draw);
        let reserved = (unit as u128) * SLASH;
        let collateral = palw_v2_collateral_for_class_set_v1(
            PalwCollateralRowV1 { declared_leaves: 7_708, derived_per_draw: floor_draw },
            &[
                PalwCollateralRowV1 { declared_leaves: 20_717_968, derived_per_draw: hy_draw },
                PalwCollateralRowV1 { declared_leaves: 27_002_967_184, derived_per_draw: dense_draw },
            ],
            GENESIS_ESCROW_SOMPI,
            None,
        );
        println!(
            "{:<14} {floor_draw:>16} {unit:>22} {reserved:>22} {:>15.2} MSK{}",
            format!("{cand:?}"),
            msk(collateral as u128),
            if cand == PALW_RC_BASE0_CANONICAL { "  <= SHIPPED" } else { "" }
        );
    }
    println!("\nposted t12 seat collateral = {} sompi = {:.2} MSK", POSTED_SOMPI, msk(POSTED_SOMPI as u128));
}

/// **The declared per-tensor dtype is a pure price multiplier.** `palw_weight_dtype_cost_v1`
/// charges F32=4, F16/BF16/I16=2, I32=4, everything else 1, per MAC. Nothing in consensus binds
/// it to the artifact: the inventory root is a walk over the artifact's weight BYTES tiled by
/// `node.tile_len` and never reads `weight_dtypes`. The only readers outside the cost model are
/// `validate_shape` (length and non-zero) and two HOST plan gates in `misaka-palw-base0`.
#[test]
fn b2x_the_declared_dtype_multiplies_the_price_and_the_collateral() {
    let f = floor();
    let floor_draw = draw(&f, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
    let rows: [(&str, PalwShapeProfileV3, (u32, u32), u64); 3] = [
        ("BASE-0 floor", floor(), PALW_RC_BASE0_CANONICAL, 7_708),
        ("Qwen3.6 v7@512", hybrid(), qwen36_held_canonical_v1(HYBRID_N_CTX), 20_717_968),
        ("Qwen2.5 A16 v7@2M", dense(), qwen25_a16_held_canonical_v1(DENSE_N_CTX), 27_002_967_184),
    ];
    println!("\n{:<20} {:>8} {:>22} {:>22} {:>9} {:>20}", "row", "dtype", "draw MAC-eq", "exposure pwu", "price x", "reserved sompi");
    for (name, base, (pf, de), leaves) in rows.iter() {
        for (label, code) in [("I8 (24)", 24u8), ("I16 (25)", 25u8), ("F16 (1)", 1u8), ("I32 (26)", 26u8)] {
            let mut p = base.clone();
            for t in [&mut p.pre_nodes, &mut p.gdn_nodes, &mut p.attn_nodes, &mut p.post_nodes] {
                for n in t.iter_mut() {
                    for dt in n.weight_dtypes.iter_mut() {
                        *dt = code;
                    }
                }
            }
            if p.validate_shape().is_err() {
                println!("{name:<20} {label:>8}  refused by validate_shape");
                continue;
            }
            let honest = draw(base, *pf, *de);
            let lied = draw(&p, *pf, *de);
            let unit = palw_exposure_unit_pwu_v1(lied, 7_708, floor_draw);
            println!(
                "{name:<20} {label:>8} {lied:>22} {unit:>22} {:>8.4}x {:>20}   (declared leaves {leaves}, unchanged)",
                lied as f64 / honest as f64,
                (unit as u128) * SLASH
            );
        }
    }
    println!("\nThe declared leaf count — the only number genesis/admission checks against the graph —");
    println!("does not move at all: `step_leaf_count` reads out_len and tile_len, never weight_dtypes.");
}

/// **Where the dtype declaration lands as fork-choice weight and as priced reward on t12.**
///
/// Past `palw_work_target` (armed at DAA 0) the ticket is `MAX·min(1, CCU/W0)` and admission
/// forces `claim.pwu == palw_pwu_v1(target, CCU)` — so below `W0` the product is flat at `W0`
/// and above it the weight IS the declared-dtype-scaled CCU.
#[test]
fn b2x_the_dtype_declaration_as_fork_weight_on_t12() {
    use kaspa_consensus_core::palw_economic_payout_v1::palw_panel_share_permille_v1;
    use kaspa_consensus_core::palw_pwu::{palw_expected_attempts_v1, palw_pwu_v1};
    use kaspa_consensus_core::palw_work_target_v1::{palw_work_floor_v1, palw_work_ticket_target_v1};

    const ESCROW_SOMPI: u64 = 320_084_650_080;
    const RATE: u64 = 900_000_000;
    let w0 = palw_work_floor_v1(ESCROW_SOMPI, RATE);
    println!("\nt12: escrow {ESCROW_SOMPI} sompi, rate {RATE} sompi/1e9 MAC-eq, W0 = {w0} MAC-eq");
    let _ = palw_panel_share_permille_v1;

    let rows: [(&str, PalwShapeProfileV3, (u32, u32)); 2] =
        [("Qwen3.6 v7@512", hybrid(), qwen36_held_canonical_v1(HYBRID_N_CTX)), ("Qwen2.5 A16 v7@2M", dense(), qwen25_a16_held_canonical_v1(DENSE_N_CTX))];
    println!("\n{:<20} {:>9} {:>22} {:>12} {:>22} {:>9}", "row", "dtype", "CCU (MAC-eq/draw)", "E[attempts]", "claim.pwu (MAC-eq)", "weight x");
    for (name, base, (pf, de)) in rows.iter() {
        let honest = draw(base, *pf, *de);
        let hp = {
            let t = palw_work_ticket_target_v1(honest, w0);
            palw_pwu_v1(t, honest.min(u64::MAX as u128) as u64) as u128
        };
        for (label, code) in [("I8 (24)", 24u8), ("I16 (25)", 25u8), ("I32 (26)", 26u8)] {
            let mut p = base.clone();
            for t in [&mut p.pre_nodes, &mut p.gdn_nodes, &mut p.attn_nodes, &mut p.post_nodes] {
                for n in t.iter_mut() {
                    for dt in n.weight_dtypes.iter_mut() {
                        *dt = code;
                    }
                }
            }
            let ccu = draw(&p, *pf, *de);
            let target = palw_work_ticket_target_v1(ccu, w0);
            let attempts = palw_expected_attempts_v1(target);
            let pwu = palw_pwu_v1(target, ccu.min(u64::MAX as u128) as u64) as u128;
            println!("{name:<20} {label:>9} {ccu:>22} {attempts:>12} {pwu:>22} {:>8.4}x", pwu as f64 / hp as f64);
        }
    }
    println!("\n(the arithmetic executed is byte-identical in every row: the cost table's dtype factor");
    println!(" is `palw_weight_dtype_cost_v1(node.weight_dtypes[layer])` and nothing else changed.)");
}
