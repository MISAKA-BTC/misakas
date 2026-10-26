//! **Why the shared-original / funded halves of T01–T15 cannot run on a node yet** (studio T4, 2026-10-22, base `g14/integration-r7`).
//!
//! Every lane the ignored campaign tests waited for is merged on r7. What still refuses them is not a missing merge but an UNADOPTED
//! VALUE: each value-carrying path resolves its numbers from a source whose only constructor is `#[cfg(test)]` in the core crate, so a
//! node build (this crate links `kaspa-consensus-core` WITHOUT `cfg(test)`) always gets the typed-missing answer. This test asserts
//! exactly that, from the node crate, so the campaign records the dependency instead of a guessed outcome. It adopts no value.
//!
//! Value numbering: `docs/design/palw/integration/mac-studio-values-and-measurements-2026-10-22.md` §2 (V1–V9, decided by the value
//! lanes). When a value lane lands a value, the matching assertion below MUST be replaced by the test that runs that path for real.
use super::*;
use kaspa_consensus_core::config::params::{
    PALW_T12_FULL_ACTIVATION_DAA, palw_t12_arm_full_activation_at_v1, palw_t12_shipped_params,
};

/// **The production value gates the ignored T tests stand behind.** Each line names the T items it blocks.
#[test]
fn t_campaign_value_gated_paths_are_refused_in_a_node_build() {
    use kaspa_consensus_core::{
        palw_court_paid_ingress_v1 as ingress, palw_court_service_binding_v1 as service, palw_court_shared_material_intake_v1 as intake,
        palw_funded_escrow_v1 as escrow, palw_funded_original_coordinator_v1 as funded, palw_state_v2::PalwTransitionExtrasV1,
        palw_tir_position_terminal_v2 as terminal,
    };
    // V1 court service profile: pinned from the user-approved estimate (2026-10-26).
    assert!(service::PALW_COURT_SERVICE_PROFILE_V1.is_some(), "V1: the approved court service profile estimate");
    // V2 funded-original admission schedule (T01/T06/T07/T09/T10/T12): every production resolution is typed-missing, so no funded
    // original is ever admitted and no `PalwFundedEscrowRowV1` is ever installed on a node.
    assert_eq!(
        funded::palw_funded_original_schedule_v1(&PalwTransitionExtrasV1::default()).err(),
        Some(funded::PalwFundedOriginalMissingV1::Schedule),
        "V2: the funded-original schedule is typed-missing in a node build"
    );
    // V3 escrow settlement dues (T01/T07/T12/T14): even a well-formed row has no fee schedule, so only the payer-return path exists.
    let row = escrow::PalwFundedEscrowRowV1::fresh_v1(
        kaspa_hashes::Hash64::from_u64_word(1),
        kaspa_hashes::Hash64::from_u64_word(2),
        Default::default(),
        1_000,
        kaspa_hashes::Hash64::from_u64_word(3),
        kaspa_hashes::Hash64::from_u64_word(4),
        10,
        10_010,
        true,
    )
    .expect("a well-formed fresh row");
    assert!(escrow::palw_escrow_fee_schedule_v1(&row).is_none(), "V3: no escrow settlement schedule exists for any row");
    // V4 paid public court ingress fee / V5 material intake: pinned from the user-approved estimates (2026-10-26).
    assert!(ingress::palw_court_ingress_schedule_v1().is_ok(), "V4: the approved paid-ingress estimate");
    assert!(intake::material_intake_schedule_v1().is_ok(), "V5: the approved material-intake estimate");
    // V8 position terminal / lookahead budgets (tags 201–204; T07/T14 terminal deadlines): not adopted.
    assert!(terminal::PALW_TIR_POSITION_WITNESS_BUDGET_V2.is_none(), "V8: no position witness budget");
    assert!(terminal::PALW_TIR_LOOKAHEAD_MAX_FRONTIER_VALUES_V2.is_none(), "V8: no lookahead frontier bound");
    // T15: the full-activation list has no height, and arming it anyway over the shipped release is refused by validation.
    assert_eq!(PALW_T12_FULL_ACTIVATION_DAA, None, "the full-activation height is not set");
    let mut armed = palw_t12_shipped_params();
    palw_t12_arm_full_activation_at_v1(&mut armed, Some(16_000));
    let refusal = armed.validate_palw_v2().expect_err("the full-activation list is refused while its arming records are unpinned");
    println!("[t-campaign value gate] full-activation@16000 refusal: {refusal:?}");
}
