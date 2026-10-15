//! **RFC-0015 §13.2 on the real node — the cases the OPV mechanics suite (`g14_opv_*`, `canonical`, `c4r3`, `recovery`) did not yet
//! carry** (2026-10-15, test source; NOT RUN in the change that wrote it — no cargo invocation was allowed; nothing here is a PASS
//! record).
//!
//! * **Old claims across the upgrade** (§11.2, §13.2 last row): a Panel-licensed claim admitted below `palw_panel_free_v1` keeps its
//!   seats, its receipts and its Panel Final after the fence; an OPV registration below the fence is dropped; one past it registers
//!   and its claim has no seat.
//! * **§8.3 on the OPV door**: where the fence carries an activation gate record (`PalwPanelFreeFenceV1::activation_gate`) an OPV
//!   claim is admitted only with a `palw_bond_budget_v1` reservation — class-independent `Q = 1`, no reward block, no Final weight —
//!   whose issuance slot returns at `d + W` and at no other time (not at Final); without the budget engine the claim is refused.
//! * **A direct proof lands over another bond's open demand** (§6.1, §13.2 "有効proofがあるが別courtが先行"): the demander is not
//!   punished, the accuser is paid, the producer is slashed once.
//!
//! Like the rest of this suite the fences are armed WITHOUT their validation (which refuses them on every height); the activation gate
//! record used here is a placeholder whose only role is to be present (the harness bypasses `validate_palw_panel_free_v1`).
use super::*;
use kaspa_consensus_core::palw_bond_budget_v1::{
    PALW_BOND_BUDGET_EXPORT_CAP_MAX_PERMILLE_V1, PALW_BOND_BUDGET_LIABILITY_HOLD_INTERIM_DAA_V1, PALW_BOND_BUDGET_POLICY_VERSION_V2,
    PALW_BUDGET_BLOCK_UNIT_V1, PalwBondBudgetFenceV1, PalwBondBudgetPolicyV1, PalwBudgetClaimRowV1, PalwBudgetOriginV1,
    PalwRoundRightsPolicyV1, PalwWeightValuePolicyV1,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_panel_free_gate_v1::{PALW_PANEL_FREE_GATE_RECORD_VERSION_V1, PalwPanelFreeGateRecordV1};

/// The upgrade height of the migration case: above every object of the Panel-licensed claim's life up to its pass.
const H: u64 = 30;
/// The common DAA hold `W` of the test budget: longer than the OPV window (50 DAA), so an early Final would show an early return.
const W: u64 = 80;

/// A placeholder record: present (so the fold's `budget_required` holds), never valid (validation is bypassed by this harness).
fn placeholder_gate() -> PalwPanelFreeGateRecordV1 {
    PalwPanelFreeGateRecordV1 {
        version: PALW_PANEL_FREE_GATE_RECORD_VERSION_V1,
        genesis: Hash64::default(),
        activation_daa: 1,
        terms: Hash64::default(),
        bond_budget_policy: Hash64::default(),
        allocation_policy: Hash64::default(),
        entries: Vec::new(),
    }
}

/// A TEST budget policy per card 0's whole collateral: ample claims, reward and weight; `W` as above.
fn test_budget_policy(unit: u64) -> PalwBondBudgetPolicyV1 {
    PalwBondBudgetPolicyV1 {
        version: PALW_BOND_BUDGET_POLICY_VERSION_V2,
        window_daa: W,
        capital_unit_sompi: unit,
        rho: 1,
        claims_per_unit: 16,
        block_units_per_unit: PALW_BUDGET_BLOCK_UNIT_V1,
        reward_per_unit_sompi: u64::MAX / 4,
        final_weight_per_unit: u64::MAX / 4,
        max_open_claims_per_bond: 1_000,
        slice_rights_by_rho: false,
        round_rights: PalwRoundRightsPolicyV1::ExecutionCap { rights_per_unit: 1_000 },
        liability_hold_daa: PALW_BOND_BUDGET_LIABILITY_HOLD_INTERIM_DAA_V1,
        export_cap_permille: PALW_BOND_BUDGET_EXPORT_CAP_MAX_PERMILLE_V1,
        weight_value: PalwWeightValuePolicyV1::Unknown,
    }
}

/// An OPV network with `fence`, the mechanics worlds' eligible class, and — where `budget` — `palw_bond_budget_v1` armed at DAA 1 with
/// [`test_budget_policy`] (mirrored onto the V2 bundle, as `budget_e2e.rs` does).
fn rfc15_config(fence: PalwPanelFreeFenceV1, budget: bool) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    opv_test_eligible(&opv_admitted());
    let (mut config, bundle, premine, floats) = kernel_config_with(Some(fence));
    if !budget {
        return (config, bundle, premine, floats);
    }
    let unit = bundle
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            Obj::BondRegistered { collateral, .. } => Some(*collateral),
            _ => None,
        })
        .expect("a genesis card");
    config.params.palw_bond_budget_v1 =
        Some(PalwBondBudgetFenceV1 { activation: ForkActivation::new(1), policy: test_budget_policy(unit) });
    config.params.sync_palw_bond_budget_v1();
    let PalwConsensusMode::ConsensusV2(armed) = &config.params.palw_consensus_mode else { unreachable!("ConsensusV2") };
    assert!(armed.state.bond_budget().is_some(), "the budget mirror is set");
    let bundle = armed.clone();
    (config, bundle, premine, floats)
}

fn gated_fence() -> PalwPanelFreeFenceV1 {
    let mut f = PalwPanelFreeFenceV1::interim_v1(ForkActivation::new(1), Vec::new());
    f.activation_gate = Some(placeholder_gate());
    f
}

/// The budget's kernel-route row of card `card`'s claims (one claim in these worlds).
fn kernel_budget_rows(w: &World, card: usize) -> Vec<PalwBudgetClaimRowV1> {
    let state = w.net.chain.tip_state().1;
    let budget = state.bond_budget.as_ref().expect("the budget engine exists past its fence");
    budget.claims.values().filter(|r| r.origin == PalwBudgetOriginV1::KernelRoute && r.bond == w.net.bond(card)).cloned().collect()
}

/// **§8.3 on the OPV door: an OPV claim reserves `Q = 1` and no reward block or Final weight (a kernel carrier creates neither); its
/// slot returns at `d + W`, not at its Final; the read shows the mode and an assurance that is not a proof.**
#[tokio::test]
async fn rfc15_e2e_an_opv_claim_draws_the_bond_budget_and_its_slot_returns_only_at_d_plus_w() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::on_opv(Net::over_cfg(rfc15_config(gated_fence(), true), TestConsensus::new)).await;
    let job = w.job().await;
    let honest = w.claim(0, &job, false).await;
    let rows = kernel_budget_rows(&w, 0);
    assert_eq!(rows.len(), 1, "one reservation for the one claim: {rows:?}");
    let row = rows[0].clone();
    assert_eq!(row.reserved.claims, 1, "Q = 1 whatever the class, its PWU or its token count");
    assert_eq!((row.reserved.block_units, row.reserved.final_weight), (0, 0), "a kernel carrier creates no reward block or weight");
    assert_eq!(row.reuse_not_before, row.accepted_daa + W, "the common hold d + W");
    assert!(row.in_window && row.open);

    let read = w.net.api().unwrap().claim_read_v1(&honest.id).unwrap().unwrap();
    assert_eq!(read.mode, "OptimisticPublicVerification");
    assert!(read.seats.is_empty() && read.quorum == 0 && read.receipts_counted == 0, "no Panel: no seat, quorum or receipt");
    let opv = read.opv.expect("the OPV clock");
    assert!(opv.statement.contains("not a proof"), "no challenge is never shown as a proof of correctness: {}", opv.statement);

    // Final at the window's end — before d + W: the issuance slot stays in the window.
    w.net.beat_to(opv.final_floor_daa).await;
    assert!(matches!(w.net.claim_state(&honest.id), ClaimStateV1::Final { .. }), "{:?}", w.net.claim_state(&honest.id));
    assert!(w.net.daa() < row.reuse_not_before, "the window outlasts the OPV Final in this world");
    let after_final = kernel_budget_rows(&w, 0);
    assert!(after_final.iter().all(|r| r.in_window), "an early Final returns nothing before d + W: {after_final:?}");

    // At d + W, and not before, the slot leaves the window — and the issuance clock frees no collateral the claim's liability still
    // holds (§8.3.3: the slot's return and the collateral's release are separate conditions).
    w.net.beat_to(row.reuse_not_before + 1).await;
    assert!(kernel_budget_rows(&w, 0).iter().all(|r| !r.in_window), "released at d + W");
    let liability_until = w.net.ledger().claims[&honest.id].liability_until;
    if liability_until.is_some_and(|until| w.net.daa() <= until) {
        assert!(w.net.kernel_reserved(0) > 0, "d + W releases the slot, never the liability collateral ({liability_until:?})");
    }
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// **Where the fence carries a gate record, an OPV claim without the budget engine is refused** (RFC-0015 §8.3: no reward-bearing
/// claim outside the class-independent caps); the job stays free for a budgeted network, nothing is written.
#[tokio::test]
async fn rfc15_e2e_with_a_gate_record_an_unbudgeted_opv_claim_is_refused() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::on_opv(Net::over_cfg(rfc15_config(gated_fence(), false), TestConsensus::new)).await;
    let job = w.job().await;
    let refused = w.claim_with(0, &job, false, Delivery::Direct).await;
    assert!(!w.net.ledger().claims.contains_key(&refused.id), "no budget engine: the OPV claim is refused at the door");
    assert_eq!(w.net.kernel_reserved(0), 0, "and reserves nothing");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// **Old claims finish under old rules** (§11.2): a Panel-licensed claim admitted below the fence keeps its seats, its receipts and its
/// Panel Final across the upgrade; an OPV registration below the fence is dropped; past the fence the OPV class registers and its claim
/// has no seat.
#[tokio::test]
async fn rfc15_e2e_a_panel_licensed_claim_admitted_below_the_fence_finishes_under_panel_rules() {
    kaspa_core::log::try_init_logger("warn");
    let fence = PalwPanelFreeFenceV1::interim_v1(ForkActivation::new(H), Vec::new());
    let mut w = World::on(Net::over_cfg(rfc15_config(fence, false), TestConsensus::new)).await;
    let job = w.job().await;
    let old = w.claim(0, &job, false).await;
    assert!(w.net.daa() < H, "the old claim is admitted below the fence");
    let seats = w.seats(&old.id);
    assert_eq!(seats.len(), 3, "a Panel-licensed claim draws its seats");

    // Below the fence an OPV registration is dropped by name.
    let d = k2_tir_v2_descriptor();
    let opv_register = K::RegisterClassV2 {
        mode: VerificationModeV1::OptimisticPublicVerification,
        descriptor: d.digest(),
        program_bytes: w.fx.program.encode(),
        plan: w.fx.plan.clone(),
        param_commitments: w.fx.pc.clone(),
    };
    let o = w.net.route(1, &opv_register);
    w.net.send(vec![(1, o)]).await;
    assert!(w.net.ledger().opv.classes.is_empty(), "no OPV class below the fence");

    w.cover(&old.id).await;
    let ClaimStateV1::ProbabilisticPass { window_end_daa, .. } = w.net.claim_state(&old.id) else { panic!("covered") };
    w.net.beat_to(H.max(window_end_daa)).await;
    let ClaimStateV1::Final { .. } = w.net.claim_state(&old.id) else { panic!("{:?}", w.net.claim_state(&old.id)) };
    let read = w.net.api().unwrap().claim_read_v1(&old.id).unwrap().unwrap();
    assert_eq!(read.mode, "PanelLicensed", "the old claim keeps its mode");
    assert!(read.opv.is_none(), "and has no OPV clock");
    assert!(w.net.daa() >= H, "Final reached past the fence, by the Panel rule it was admitted under");
    let finals = w.net.api().unwrap().finals_read_v1().unwrap();
    let old_final = finals.iter().find(|f| f.receipt.claim == old.id).expect("the old claim's Final");
    assert_eq!(old_final.final_path, "PanelLicensed", "never re-labelled Panel-independent");

    // Past the fence the OPV class registers; its claim has no seat.
    let o = w.net.route(1, &opv_register);
    w.net.send(vec![(1, o)]).await;
    let opv_class =
        single_class_id_v1(d.digest(), &w.fx.program.encode(), &w.fx.plan, &w.fx.pc, VerificationModeV1::OptimisticPublicVerification);
    assert!(w.net.ledger().opv.classes.contains(&opv_class), "the OPV class registers past the fence");
    let legacy_class = w.class;
    w.class = opv_class;
    let opv_job = w.job().await;
    let new = w.claim(0, &opv_job, false).await;
    assert!(w.net.claim_state(&new.id).to_string_state().starts_with("Challengeable"), "{:?}", w.net.claim_state(&new.id));
    let read = w.net.api().unwrap().claim_read_v1(&new.id).unwrap().unwrap();
    assert!(read.seats.is_empty(), "an OPV claim has no seat");
    assert_ne!(legacy_class, opv_class, "the same program under the other mode is another class");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// **A direct proof lands over another bond's open demand on an OPV claim**: the demander is not punished, the accuser is paid its
/// share once, the producer loses its reservation once.
#[tokio::test]
async fn rfc15_e2e_a_direct_proof_lands_over_another_bonds_open_demand() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::opv().await;
    let job = w.job().await;
    let before = w.net.collateral(0);
    let lie = w.claim(0, &job, true).await;
    let out = w.outsiders(&lie, &[], 2);
    let (demander, accuser) = (out[0], out[1]);
    let demander_slashed = w.net.slashed(demander);
    w.demand(demander, &lie.id, 0).await;
    assert!(w.net.ledger().demands.keys().any(|(c, ..)| *c == lie.id), "the first bond's demand is open");

    let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0x61);
    w.proof(accuser, &lie.id, proof).await;
    assert!(w.net.ledger().claims[&lie.id].convicted, "the open demand does not pre-empt the objective proof");
    let (policy, _) = opv_view(&w, &lie.id);
    let slashed = policy.economics.reservation_per_claim;
    assert_eq!(w.net.collateral(0), before - slashed - policy.economics.admission_fee, "slashed once");
    assert_eq!(w.net.owed(accuser), slashed * u64::from(w.policy().accuser_reward_permille) / 1000, "the accuser's share, once");
    assert_eq!(w.net.slashed(demander), demander_slashed, "the legitimate demander is not punished");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// **Concurrent OPV claims of one bond never share a collateral** (§8.1): each reserves its own `reservation_per_claim`; a conviction
/// slashes only the lying claim's reservation, the honest one finalizes with its reservation still held for its liability.
#[tokio::test]
async fn rfc15_e2e_concurrent_opv_claims_reserve_separately_and_a_conviction_takes_only_its_own() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::opv().await;
    let (job_a, job_b) = (w.job().await, w.job().await);
    let before = w.net.collateral(0);
    let honest = w.claim(0, &job_a, false).await;
    let lie = w.claim(0, &job_b, true).await;
    let (policy, view) = opv_view(&w, &honest.id);
    let r = policy.economics.reservation_per_claim;
    assert_eq!(w.net.kernel_reserved(0), 2 * u128::from(r), "two claims, two reservations: no double use of one collateral");
    let outsider = w.outsiders(&lie, &[], 1)[0];
    let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0x71);
    w.proof(outsider, &lie.id, proof).await;
    assert!(w.net.ledger().claims[&lie.id].convicted && !w.net.ledger().claims[&honest.id].convicted);
    assert_eq!(w.net.collateral(0), before - r - 2 * policy.economics.admission_fee, "only the lie's reservation is slashed");
    assert_eq!(w.net.kernel_reserved(0), u128::from(r), "the honest claim's reservation is untouched");
    w.net.beat_to(view.final_floor_daa).await;
    assert!(matches!(w.net.claim_state(&honest.id), ClaimStateV1::Final { .. }), "{:?}", w.net.claim_state(&honest.id));
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// **A fake fraud bounty loop loses money** (§8.2): a producer that convicts its own lie through another bond it controls gets back
/// at most the reporter share of the collected slash; together the two bonds end poorer than they started.
#[tokio::test]
async fn rfc15_e2e_a_self_prosecuted_lie_costs_the_producer_and_its_sybil_together() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::opv().await;
    let job = w.job().await;
    let lie = w.claim(0, &job, true).await;
    let sybil = w.outsiders(&lie, &[], 1)[0];
    let together_before = w.net.collateral(0) + w.net.collateral(sybil);
    let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0x72);
    w.proof(sybil, &lie.id, proof).await;
    assert!(w.net.ledger().claims[&lie.id].convicted);
    let together_after = w.net.collateral(0) + w.net.collateral(sybil) + w.net.owed(sybil);
    assert!(together_after < together_before, "the loop burns the non-reporter share: {together_after} < {together_before}");
    let (policy, _) = opv_view(&w, &lie.id);
    let burned = policy.economics.reservation_per_claim - w.net.owed(sybil);
    assert!(burned > 0 && w.net.owed(sybil) < policy.economics.reservation_per_claim, "the bounty is a share, never the whole slash");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}
