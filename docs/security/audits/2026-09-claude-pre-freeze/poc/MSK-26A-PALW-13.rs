//! MSK-26A-PALW-13 — palw_model_benefits_is_strengthening_v1 ignores cadence_daa and holders
//! between thresholds, so a line owner can switch off the enforced early-access lead in one block
//! without the 4,000-DAA notice and restore it in the next.
//!
//! Audit commit: 3d2bd6dc5d77d37396d1b73c4c13526090923b92
//! Crate:        kaspa-consensus-core (consensus/core)
//! Command:
//!   cp docs/security/audits/2026-09-claude-pre-freeze/poc/MSK-26A-PALW-13.rs \
//!      consensus/core/tests/audit_poc_msk_26a_palw_13.rs \
//!   && cargo test -p kaspa-consensus-core --test audit_poc_msk_26a_palw_13 -- --nocapture ; \
//!   rm consensus/core/tests/audit_poc_msk_26a_palw_13.rs
//!
//! PASS = the vulnerable behaviour is present:
//!   * `cadence_imposition_zeroes_the_enforced_lead_without_notice`: after a lead of 4,000 DAA is
//!     in effect and a version is published as a preview, the owner re-declares the SAME tiers with
//!     `cadence_daa = 1`. The fold classifies it as a strengthening (no `pending`, no notice), the
//!     declaration lapses immediately, the enforced lead reads 0, and `ModelVersionPromoted` is
//!     accepted 3 DAA after the preview was published (the ADR-0095 §4.4/§4.7 behaviour would
//!     refuse it until published + 4,000). A straight-to-current publish is accepted too, and a
//!     re-declaration with cadence 0 restores the 4,000-DAA lead in its own block.
//!     The CONTROL in the same test shows the notice machinery DOES catch the direct A9 route
//!     (lowering the lead): that one is stored as pending and the promotion is still refused.
//!   * `intermediate_no_grant_tier_is_classified_as_strengthening`: adding a tier with zero grants
//!     above the existing threshold lands at once and a real holder at that threshold loses every
//!     grant in the chain's own tier reader (`model_benefit_tier`), with no notice.
//!
//! The fold (`apply_palw_transition_v2_with_extras`) is driven directly; the model-registry flags
//! are resolved from `palw_t12_shipped_params()` at the block's DAA exactly as the processor's
//! `palw_transition_extras_for` resolves them. The processor itself checks only the fence and the
//! owner/developer signature for these three objects (processor.rs:8741-8817), and the fold does
//! not verify signatures, so `signature: vec![1]` stands for the owner's / developer's real one.

use kaspa_consensus_core::config::params::{mainnet_shipped_params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_model_benefits_v1::{
    PALW_MODEL_BENEFIT_NOTICE_DAA, PalwModelBenefitTierV1, grant, palw_model_benefits_enforced_lead_v1,
    palw_model_benefits_is_strengthening_v1, tier_for_units,
};
use kaspa_consensus_core::palw_model_lines_v1::model_line_id_v1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2 as Obj, PalwPwuRuleV2, PalwStateParamsV2,
    PalwStateV2Error, PalwTransitionExtrasV1, apply_palw_transition_v2_with_extras,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}
fn bond(n: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(n), index: 0 })
}

const BASE: u64 = 0xBA5E;
const FLOOR_ROOT: u64 = 0xF100;
const CLASS: u64 = 0xC0DE;
const CLASS_ROOT: u64 = 0xA271_FAC7;
const LINE_ROOT: u64 = 0x1111_0001;
const OWNER: u64 = 0xB0;

fn params() -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 10, 10, 20, 500, 1_000, h(BASE), 4, 1_000, 100, 1_000, 0)
        .expect("state params")
        .with_claim_retirement_daa(3_000)
        .expect("retirement")
        .with_min_base_class_share_permille(20)
        .expect("floor reserve")
}

fn registration(class: u64, root: u64, share: u16) -> Obj {
    Obj::ClassRegistered {
        class_id: h(class),
        artifact_root: h(root),
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 7_708 },
        initial_target: u128::MAX / 2,
        share_permille: share,
        activation_daa: 0,
        admission: None,
    }
}

fn bond_object(n: u64) -> Obj {
    Obj::BondRegistered {
        bond: bond(n),
        pubkey: vec![n as u8; 32],
        operator_pubkey: vec![(n as u8).wrapping_add(21); 8],
        collateral: 1_000_000_000_000_000,
        payout_payload: h(0x9A4 + n),
        capable_classes: Default::default(),
        signature: vec![9u8; 64],
    }
}

fn tier(min_units: u64, grants: u32, lead_daa: u64, min_hold_daa: u64) -> PalwModelBenefitTierV1 {
    PalwModelBenefitTierV1 { min_units, grants, lead_daa, min_hold_daa, note: Vec::new() }
}

fn declare(line: Hash64, tiers: Vec<PalwModelBenefitTierV1>, cadence_daa: u64, expires_daa: u64) -> Obj {
    Obj::ModelLineBenefitsDeclared { line_id: line, tiers, cadence_daa, expires_daa, signature: vec![1] }
}

fn publish(line: Hash64, version: u32, root: Hash64, preview: bool) -> Obj {
    Obj::ModelVersionPublished {
        line_id: line,
        version,
        root,
        parent: Some(version - 1),
        adopted_from: None,
        runtime_hash: Some(h(0xF0)),
        dataset_commitment: None,
        training_config_hash: None,
        notes_hash: None,
        preview,
        signature: vec![1],
    }
}

fn promote(line: Hash64, version: u32) -> Obj {
    Obj::ModelVersionPromoted { line_id: line, version, signature: vec![1] }
}

/// The registry flags exactly as testnet-12's shipped params resolve them at `daa`.
fn t12_extras(daa: u64) -> PalwTransitionExtrasV1 {
    let p = palw_t12_shipped_params();
    PalwTransitionExtrasV1 {
        model_lines_active: p.palw_model_lines_active_at(daa),
        model_benefits_active: p.palw_model_benefits_active_at(daa),
        artifact_root_ownership_active: p.palw_artifact_root_ownership_at(daa),
        ..Default::default()
    }
}

#[derive(Clone)]
struct Chain {
    state: PalwChainStateV2,
    params: PalwStateParamsV2,
    daa: u64,
}

impl Chain {
    fn try_at(&self, daa: u64, objects: &[Obj]) -> Result<PalwChainStateV2, PalwStateV2Error> {
        let ctx = PalwBlockContextV2 { block: h(daa | 0x1000_0000), daa_score: daa, blue_score: daa, subsidy: 1_000_000 };
        apply_palw_transition_v2_with_extras(&self.state, &self.params, &ctx, objects, None, false, false, false, false, &t12_extras(daa))
            .map(|(s, _)| s)
    }
    /// Try one block at the next DAA; on success commit it.
    fn step(&mut self, objects: &[Obj]) -> Result<(), PalwStateV2Error> {
        self.step_to(self.daa + 1, objects)
    }
    fn step_to(&mut self, daa: u64, objects: &[Obj]) -> Result<(), PalwStateV2Error> {
        assert!(daa > self.daa);
        let next = self.try_at(daa, objects)?;
        self.state = next;
        self.daa = daa;
        Ok(())
    }

    /// Two Active bonds, the floor, a class, and a line founded on it by bond OWNER, who is its
    /// owner and developer (ADR-0088 Decision 1).
    fn with_line() -> (Self, Hash64) {
        let mut c = Chain { state: PalwChainStateV2::genesis(), params: params(), daa: 0 };
        c.step(&[bond_object(OWNER), bond_object(0xB1), registration(BASE, FLOOR_ROOT, 1_000)]).expect("genesis folds");
        c.step(&[registration(CLASS, CLASS_ROOT, 1)]).expect("class registered");
        let found = Obj::ModelLineFounded { class_id: h(CLASS), name: b"LINE".to_vec(), founder: bond(OWNER), root: h(LINE_ROOT), signature: vec![1] };
        c.step(&[found]).expect("line founded");
        let line = model_line_id_v1(&h(CLASS), &bond(OWNER), b"LINE");
        let row = c.state.model_line_or_founding(&line).expect("the line exists");
        assert_eq!(row.owner, Some(bond(OWNER)));
        (c, line)
    }
}

fn print_reachability() {
    let t12 = palw_t12_shipped_params();
    let main = mainnet_shipped_params();
    println!("palw_t12_shipped_params(): palw_model_lines = {:?}, palw_model_benefits = {:?}", t12.palw_model_lines, t12.palw_model_benefits);
    println!(
        "palw_t12_shipped_params(): model_lines_active_at(0) = {}, model_benefits_active_at(0) = {}",
        t12.palw_model_lines_active_at(0),
        t12.palw_model_benefits_active_at(0)
    );
    println!("mainnet_shipped_params(): palw_model_lines = {:?}, palw_model_benefits = {:?}", main.palw_model_lines, main.palw_model_benefits);
    println!(
        "mainnet_shipped_params(): model_lines_active_at(10^9) = {}, model_benefits_active_at(10^9) = {}",
        main.palw_model_lines_active_at(1_000_000_000),
        main.palw_model_benefits_active_at(1_000_000_000)
    );
}

#[test]
fn cadence_imposition_zeroes_the_enforced_lead_without_notice() {
    print_reachability();
    assert!(palw_t12_shipped_params().palw_model_benefits_active_at(0), "testnet-12 arms ADR-0095 from DAA 0");

    let (mut c, line) = Chain::with_line();
    const LEAD: u64 = 4_000;
    let promise = vec![tier(1, grant::EARLY_VERSION, LEAD, 0)];

    // 1. The owner declares a 4,000-DAA early-access lead, no cadence undertaking.
    c.step(&[declare(line, promise.clone(), 0, 0)]).expect("declaration lands");
    assert_eq!(c.state.model_benefit_enforced_lead(&line, c.daa), LEAD);
    println!("DAA {}: declared tiers [min 1, EARLY_VERSION, lead {LEAD}] cadence 0 -> enforced lead {}", c.daa, LEAD);

    // 2. Control: straight to current is refused while the lead is in effect.
    let direct = c.try_at(c.daa + 1, &[publish(line, 2, h(0xB2), false)]).unwrap_err();
    println!("DAA {}: publish v2 straight to current -> {direct:?}", c.daa + 1);
    assert!(matches!(direct, PalwStateV2Error::ModelBenefitMustEnterAsPreview { lead_daa: LEAD, .. }));

    // v2 enters as a preview: the holders' window opens.
    c.step(&[publish(line, 2, h(0xB2), true)]).expect("preview publish");
    let published = c.daa;
    assert_eq!(c.state.model_line_last_version_daa(&line), published);

    // Control: an early promotion is refused with promotable_at = published + 4,000.
    let early = c.try_at(published + 1, &[promote(line, 2)]).unwrap_err();
    println!("DAA {}: promote v2 -> {early:?}", published + 1);
    assert!(
        matches!(early, PalwStateV2Error::ModelBenefitLeadNotElapsed { version: 2, promotable_at, .. } if promotable_at == published + LEAD)
    );

    // CONTROL (A9): lowering the lead directly IS a weakening -> pending, and promotion still refused.
    {
        let mut a9 = c.clone();
        a9.step_to(published + 2, &[declare(line, vec![tier(1, grant::EARLY_VERSION, 1, 0)], 0, 0)]).expect("A9 declaration lands");
        let row = a9.state.model_benefits(&line).unwrap().clone();
        assert!(row.pending.is_some(), "a lowered lead is stored as pending");
        let refused = a9.try_at(published + 3, &[promote(line, 2)]).unwrap_err();
        println!("CONTROL (lower lead to 1): pending = {:?}; promote at DAA {} -> {refused:?}", row.pending.as_ref().map(|p| p.effective_daa), published + 3);
        assert!(matches!(refused, PalwStateV2Error::ModelBenefitLeadNotElapsed { .. }));
    }

    // 3. Two DAA after the preview the owner re-declares the IDENTICAL tiers with cadence 1.
    let pure = palw_model_benefits_is_strengthening_v1(&promise, 0, &promise, 0);
    println!("palw_model_benefits_is_strengthening_v1(prev, 0, same tiers, 0) = {pure} (the function has no cadence argument)");
    assert!(pure);
    c.step_to(published + 2, &[declare(line, promise.clone(), 1, 0)]).expect("cadence-only re-declaration lands");
    let row = c.state.model_benefits(&line).unwrap().clone();
    println!(
        "DAA {}: re-declared same tiers with cadence 1 -> row.cadence_daa = {}, row.pending = {:?}",
        c.daa, row.cadence_daa, row.pending
    );
    assert_eq!(row.cadence_daa, 1, "the new cadence governs at once");
    assert!(row.pending.is_none(), "BUG: no notice was imposed");
    assert_eq!(row.tiers, promise);

    // 4. The declaration is lapsed immediately, so the enforced lead is 0.
    let lead_now = c.state.model_benefit_enforced_lead(&line, c.daa);
    let lapse = c.state.model_benefit_lapse(&line, c.daa);
    println!("DAA {}: enforced lead = {lead_now}, lapse = {lapse:?}", c.daa);
    assert_eq!(lead_now, 0, "BUG: the 4,000-DAA lead is gone without notice");
    assert!(lapse.is_some());

    // 5a. A straight-to-current publish is accepted now (alternate branch).
    {
        let direct_ok = c.try_at(published + 3, &[publish(line, 3, h(0xB3), false)]);
        println!("DAA {}: publish v3 straight to current -> {}", published + 3, if direct_ok.is_ok() { "ACCEPTED" } else { "refused" });
        let s = direct_ok.expect("BUG: straight-to-current publish accepted while the promise should still bind");
        assert_eq!(s.model_line_or_founding(&line).unwrap().current, 3);
    }

    // 5b. The preview is promoted 3 DAA after it was published, 3,997 DAA before the promise allows.
    c.step_to(published + 3, &[promote(line, 2)]).expect("BUG: early promotion accepted");
    let current = c.state.model_line_or_founding(&line).unwrap().current;
    println!(
        "DAA {}: promote v2 -> ACCEPTED, line.current = {current} (promise said not before DAA {})",
        c.daa,
        published + LEAD
    );
    assert_eq!(current, 2);
    assert!(c.daa < published + LEAD);
    assert!(c.daa < published + 2 + PALW_MODEL_BENEFIT_NOTICE_DAA);

    // 6. The owner restores cadence 0: against an empty live set every declaration strengthens.
    c.step(&[declare(line, promise.clone(), 0, 0)]).expect("restore lands");
    let row = c.state.model_benefits(&line).unwrap().clone();
    let restored = c.state.model_benefit_enforced_lead(&line, c.daa);
    println!("DAA {}: re-declared with cadence 0 -> pending = {:?}, enforced lead = {restored}", c.daa, row.pending);
    assert!(row.pending.is_none());
    assert_eq!(restored, LEAD, "the lead card is back one block later, as if nothing happened");
}

#[test]
fn intermediate_no_grant_tier_is_classified_as_strengthening() {
    use kaspa_consensus_core::palw_model_market_v1::PALW_MODEL_SEED_MIN_SOMPI_V1;
    const MSK: u64 = 100_000_000;
    let (mut c, line) = Chain::with_line();
    let who = h(0xB0_0001);

    // A holder with a real position on the line.
    let seed = Obj::ModelSeed { line_id: line, seeder: h(0xB0_0009), msk_seed: PALW_MODEL_SEED_MIN_SOMPI_V1, sink_index: 1 };
    let buy = Obj::ModelBuy { line_id: line, holder: who, msk_in: 1_000 * MSK, min_units_out: 0, sink_index: 1 };
    c.step(&[seed]).expect("seed");
    c.step(&[buy]).expect("buy");
    let held = c.state.model_position(&line, &who);
    assert!(held > 1, "holder bought units");

    let both = grant::EARLY_VERSION | grant::SUPPORT;
    let prev = vec![tier(1, both, 600, 0)];
    c.step(&[declare(line, prev.clone(), 0, 0)]).expect("declaration lands");
    let before = c.state.model_benefit_tier(&line, &who, c.daa).expect("the holder is a member");
    println!("DAA {}: holder of {held} units -> tier {:?} grants {:?}", c.daa, before.0, grant::names_of(before.1.grants));
    assert_eq!(before.1.grants, both);

    // The owner inserts a tier at the holder's balance that grants nothing.
    let next = vec![tier(1, both, 600, 0), tier(held, 0, 0, 0)];
    let pure = palw_model_benefits_is_strengthening_v1(&prev, 0, &next, 0);
    println!(
        "is_strengthening(prev, next) = {pure}; tier_for_units(prev, {held}) grants {:?}, tier_for_units(next, {held}) grants {:?}",
        grant::names_of(tier_for_units(&prev, held, u64::MAX).unwrap().grants),
        grant::names_of(tier_for_units(&next, held, u64::MAX).unwrap().grants)
    );
    assert!(pure, "BUG: a tier that strips a holder of every grant is a 'strengthening'");

    c.step(&[declare(line, next.clone(), 0, 0)]).expect("re-declaration lands");
    let row = c.state.model_benefits(&line).unwrap().clone();
    assert!(row.pending.is_none(), "BUG: no notice");
    let after = c.state.model_benefit_tier(&line, &who, c.daa).expect("still inside the ladder");
    println!(
        "DAA {}: pending = {:?}; holder of {held} units -> tier {:?} grants {:?}",
        c.daa,
        row.pending,
        after.0,
        grant::names_of(after.1.grants)
    );
    assert_eq!(after.1.grants, 0, "BUG: the holder lost EARLY_VERSION and SUPPORT in the same block");

    // Scope note: this variant does NOT lower the consensus-enforced lead (it is the max over
    // EARLY_VERSION tiers, and prev's tier is kept), only what the tier reader grants a holder.
    let lead = palw_model_benefits_enforced_lead_v1(&c.state.model_benefit_tiers_in_effect(&line, c.daa));
    println!("enforced lead after the variant = {lead} (unchanged)");
    assert_eq!(lead, 600);
}
