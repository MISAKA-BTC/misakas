//! REQ tests of the 2026-10-12 rulings' dormant rules (register X3, N4, D2). Unverified source (2026-10-15): written, not run.

use super::*;
use crate::config::params::ForkActivation;
use crate::network::{NetworkId, NetworkType};
use crate::palw_court_scope_v1::{CourtUnitKindV1 as K, palw_court_demand_allowed_v1};
use crate::palw_freeprompt_v3::{PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V4_VERSION, PalwFpStopReasonV3};
use crate::palw_state_v2::{PalwChainStateV2, PalwClaimPhaseV2, palw_rcore_deadline_v1};

const FENCE: u64 = 1_000;

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

/// testnet-12's bundle state params, as shipped (every ruling mirror `None`).
fn shipped_sp() -> PalwStateParamsV2 {
    match t12().palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle.state,
        _ => panic!("testnet-12 is V2"),
    }
}

/// An X3 fence whose spans are the composition of testnet-12's windows at [`FENCE`] and whose five `B_*` terms and `c_retain` are the
/// probe's NON-ZERO fixture values (stand-ins for measurements in this test only; no shipped value is measured).
fn composed_fence() -> PalwClaimHardDeadlineFenceV1 {
    let activation = ForkActivation::new(FENCE);
    let base = PalwClaimHardDeadlineFenceV1::unapproved_probe_at(activation).policy;
    PalwClaimHardDeadlineFenceV1 { activation, policy: palw_x3_composed_policy_v1(&shipped_sp(), activation, base) }
}

fn policy() -> PalwClaimHardDeadlinePolicyV1 {
    PalwClaimHardDeadlineFenceV1::unapproved_probe_at(ForkActivation::new(FENCE)).policy
}

/// The shipped state params with all three rulings armed at [`FENCE`] (a fixture: no shipped ruleset can carry this).
fn armed_sp() -> PalwStateParamsV2 {
    shipped_sp()
        .with_court_model_scope_from_daa(Some(FENCE))
        .with_public_evidence_reward_from_daa(Some(FENCE))
        .with_claim_hard_deadline_v1(Some(PalwClaimHardDeadlineMirrorV1 { from_daa: FENCE, policy: policy() }))
}

/// A claim (every other field zero) accepted at `accepted`, owing its trace through `retention`.
fn claim(accepted: u64, retention: u64) -> PalwClaimStateV2 {
    let zeros = [0u8; 8192];
    let mut c = <PalwClaimStateV2 as BorshDeserialize>::deserialize(&mut &zeros[..]).expect("a zero claim decodes");
    c.accepted_daa = accepted;
    c.trace_retention_daa = retention;
    c
}

fn job(privacy_mode: u8, version: u16) -> PalwFreePromptJobV3 {
    PalwFreePromptJobV3 {
        version,
        network_domain: Hash64::from_u64_word(0x4E45_5457),
        class_id: Hash64::from_u64_word(0xC1),
        executor_bond: crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_bytes([1u8; 64]), 0),
        executor_pubkey: vec![7u8; 32],
        operator_id: Hash64::from_u64_word(0xE0),
        anchor_block: Hash64::from_u64_word(0xA0),
        anchor_daa: 5_000,
        job_nonce: [0x11; 32],
        tokenizer_id: Hash64::from_u64_word(0x70),
        prompt_token_ids_hash: Hash64::from_u64_word(0x9012),
        prompt_tokens: 96,
        decode_token_limit: 128,
        max_context_tokens: 4096,
        privacy_mode,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: crate::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
        temperature_q: crate::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        decode: None,
        tail: None,
    }
}

fn commitment(job: PalwFreePromptJobV3) -> PalwFreePromptCommitmentV3 {
    PalwFreePromptCommitmentV3 {
        job,
        trace_root: Hash64::from_u64_word(1),
        output_root: Hash64::from_u64_word(2),
        schedule_root: Hash64::from_u64_word(3),
        execution_root: Hash64::from_u64_word(4),
        decode_tokens_executed: 8,
        stop_reason: PalwFpStopReasonV3::ExactBudgetReached,
        work_leaves: 64,
        trace_manifest_root: Hash64::from_u64_word(5),
        trace_chunk_count: 1,
        trace_retention_daa: 10_000,
    }
}

// ---- the fences as shipped -------------------------------------------------------------------------------------------------------

#[test]
fn every_shipped_ruleset_mirrors_nothing_so_every_rule_is_the_old_one() {
    let sp = shipped_sp();
    assert_eq!(sp.court_model_scope_from_daa(), None);
    assert_eq!(sp.public_evidence_reward_from_daa(), None);
    assert_eq!(sp.claim_hard_deadline_v1(), None);
    let late = claim(u64::MAX / 2, u64::MAX / 2 + 10_000);
    assert!(!palw_claim_x3_governs_v1(&sp, &late));
    assert_eq!(palw_claim_hard_deadline_of_v1(&sp, &late), None);
    assert_eq!(palw_claim_pursuit_may_start_v1(&sp, &late, u64::MAX), Ok(()));
    assert_eq!(palw_public_evidence_reward_refusal_v1(&sp, u64::MAX, None), None);
    for kind in PALW_D2_RESTRICTED_UNITS_V1 {
        assert_eq!(palw_court_model_scope_admits_v1(&sp, &late, kind), Ok(()));
    }
    assert_eq!(palw_court_model_scope_admits_readiness_v1(&sp, u64::MAX, Some(u64::MAX)), Ok(()));
    t12().validate_palw_2026_10_12_ruling_mirrors_v1().expect("shipped mirrors agree with the shipped fences");
}

#[test]
fn the_sync_writes_the_mirrors_and_validation_refuses_a_mirror_that_disagrees() {
    let mut p = t12();
    p.palw_court_model_scope_v1 = Some(ForkActivation::new(FENCE));
    p.palw_public_evidence_reward_v1 = Some(ForkActivation::new(FENCE));
    p.palw_claim_hard_deadline_v1 = Some(PalwClaimHardDeadlineFenceV1::unapproved_probe_at(ForkActivation::new(FENCE)));
    let unsynced = p.validate_palw_2026_10_12_ruling_mirrors_v1().expect_err("the bundle still mirrors nothing");
    assert!(format!("{unsynced:?}").contains("palw_court_model_scope_v1"), "{unsynced:?}");
    p.sync_palw_2026_10_12_ruling_fences_v1();
    p.validate_palw_2026_10_12_ruling_mirrors_v1().expect("synced");
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { unreachable!() };
    assert_eq!(bundle.state.court_model_scope_from_daa(), Some(FENCE));
    assert_eq!(bundle.state.public_evidence_reward_from_daa(), Some(FENCE));
    assert_eq!(bundle.state.claim_hard_deadline_v1(), Some(PalwClaimHardDeadlineMirrorV1 { from_daa: FENCE, policy: policy() }));
    // Still refused as an arming: the rules are unverified and no X3 value is approved.
    assert!(p.validate_palw_2026_10_12_ruling_fences_v1().is_err());
    // `never()` mirrors nothing.
    let mut never = t12();
    never.palw_claim_hard_deadline_v1 = Some(PalwClaimHardDeadlineFenceV1::unapproved_probe_at(ForkActivation::never()));
    never.palw_court_model_scope_v1 = Some(ForkActivation::never());
    never.sync_palw_2026_10_12_ruling_fences_v1();
    never.validate_palw_2026_10_12_ruling_fences_v1().expect("never() is dormant");
}

#[test]
fn the_x3_policy_id_moves_with_every_term_and_the_structural_refusals_hold() {
    let base = policy();
    base.validate().expect("the probe policy is structurally sound");
    let mut seen = std::collections::BTreeSet::new();
    assert!(seen.insert(base.id()));
    let bumps: [fn(&mut PalwClaimHardDeadlinePolicyV1); 9] = [
        |p| p.prepare_daa += 1,
        |p| p.observe_daa += 1,
        |p| p.resolve_daa += 1,
        |p| p.cold_fetch_daa += 1,
        |p| p.check_daa += 1,
        |p| p.localize_daa += 1,
        |p| p.carrier_daa += 1,
        |p| p.reorg_slack_daa += 1,
        |p| p.retention_cost_per_daa_sompi += 1,
    ];
    for bump in bumps {
        let mut moved = base;
        bump(&mut moved);
        assert!(seen.insert(moved.id()), "every term is in the policy id: {moved:?}");
    }
    assert!(PalwClaimHardDeadlinePolicyV1 { resolve_daa: 0, ..base }.validate().is_err());
    assert!(PalwClaimHardDeadlinePolicyV1 { version: 2, ..base }.validate().is_err());
    assert!(PalwClaimHardDeadlinePolicyV1 { localize_daa: u64::MAX, ..base }.validate().is_err());
}

// ---- X3 ------------------------------------------------------------------------------------------------------------------------

/// REQ-X3-1: the hard deadline is a pure function of the claim's acceptance (d0) and the fence's policy — fixed at acceptance, not at
/// the first pursuit, and no longer capped by retention (REQ-X3-6 raises retention to it instead); a claim accepted below the fence
/// has none (old ruleset).
#[test]
fn req_x3_1_the_hard_deadline_is_fixed_at_acceptance() {
    let sp = armed_sp();
    let p = policy();
    assert_eq!(palw_claim_hard_deadline_of_v1(&sp, &claim(FENCE - 1, 1_000_000)), None, "accepted below the fence: old ruleset");
    let mut c = claim(FENCE, 1_000_000);
    let d = palw_claim_deadlines_v1(&sp, &c).expect("accepted at the fence: X3");
    assert_eq!(d.receipt_deadline_daa, FENCE + p.prepare_daa);
    assert_eq!(d.open_cutoff_daa, FENCE + p.prepare_daa + p.observe_daa);
    assert_eq!(d.hard_deadline_daa, FENCE + p.prepare_daa + p.observe_daa + p.resolve_daa);
    // Nothing after acceptance moves it: not a licence, a redraw, or a later licence anchor.
    for phase in [
        PalwClaimPhaseV2::Provisional,
        PalwClaimPhaseV2::PanelBound { bound_daa: FENCE + 30 },
        PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: FENCE + 39 },
        PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: FENCE + 500 },
    ] {
        c.phase = phase;
        c.rebound_daa = Some(FENCE + 20);
        assert_eq!(palw_claim_deadlines_v1(&sp, &c), Some(d), "the deadline is the claim's, fixed at d0");
    }
    // Lead default (2026-10-15, round 3): a short retention no longer caps the hard deadline — the retention follows it (REQ-X3-6).
    let short = claim(FENCE, FENCE + 50);
    assert_eq!(palw_claim_hard_deadline_of_v1(&sp, &short), Some(FENCE + p.prepare_daa + p.observe_daa + p.resolve_daa));
    assert_eq!(palw_x3_hard_deadline_at_v1(&p, FENCE), d.hard_deadline_daa);
}

/// REQ-X3-6 (Lead default 2026-10-15, round 3): an X3 claim's material is owed through its hard deadline — `retention = max(retention,
/// hard)`, raised once by the fold's claim writer at creation — and the extension is priced by the policy's named per-DAA retention
/// cost (`c_retain`), with an unmeasured cost refused rather than read as free. Old claims: `None`, the record unchanged.
#[test]
fn req_x3_6_retention_extends_to_the_hard_deadline_and_is_priced() {
    let sp = armed_sp();
    let p = policy();
    let hard = FENCE + p.prepare_daa + p.observe_daa + p.resolve_daa;
    // Short retention: raised to hard. Long retention: kept.
    assert_eq!(palw_claim_x3_retention_daa_v1(&sp, &claim(FENCE, FENCE + 50)), Some(hard));
    assert_eq!(palw_claim_x3_retention_daa_v1(&sp, &claim(FENCE, 1_000_000)), Some(1_000_000));
    assert_eq!(palw_claim_retention_obligation_v1(&sp, &claim(FENCE, FENCE + 50)), hard);
    // Old claims and shipped rulesets: no obligation beyond the record's own.
    assert_eq!(palw_claim_x3_retention_daa_v1(&sp, &claim(FENCE - 1, FENCE + 50)), None);
    assert_eq!(palw_claim_retention_obligation_v1(&sp, &claim(FENCE - 1, FENCE + 50)), FENCE + 50);
    assert_eq!(palw_claim_x3_retention_daa_v1(&shipped_sp(), &claim(u64::MAX / 2, 7)), None);
    // Every pursuit X3 admits now runs inside retention: `start + B_remaining <= hard <= retention`.
    let c = claim(FENCE, FENCE + 50);
    let budget = palw_claim_pursuit_budget_v1(&sp, &p);
    for start in [FENCE, FENCE + 10, FENCE + p.prepare_daa + p.observe_daa] {
        if palw_claim_pursuit_may_start_v1(&sp, &c, start).is_ok() {
            assert!(start + budget <= palw_claim_retention_obligation_v1(&sp, &c));
        }
    }

    // The priced extension, in numbers.
    let ext = palw_x3_retention_extension_at_v1(&p, FENCE, FENCE + 50);
    assert_eq!(ext, PalwX3RetentionExtensionV1 { base_retention_daa: FENCE + 50, retention_daa: hard, extra_daa: hard - FENCE - 50 });
    assert_eq!(palw_x3_retention_extension_at_v1(&p, FENCE, hard + 9).extra_daa, 0, "no extension past hard");
    assert_eq!(palw_x3_retention_cost_sompi_v1(&p, ext.extra_daa), Ok(ext.extra_daa * p.retention_cost_per_daa_sompi));
    assert_eq!(palw_x3_retention_cost_sompi_v1(&p, 0), Ok(0));
    let unmeasured = PalwClaimHardDeadlinePolicyV1 { retention_cost_per_daa_sompi: 0, ..p };
    assert!(palw_x3_retention_cost_sompi_v1(&unmeasured, 1).is_err(), "an unmeasured cost is not free");
    assert!(
        palw_x3_retention_cost_sompi_v1(&PalwClaimHardDeadlinePolicyV1 { retention_cost_per_daa_sompi: u64::MAX, ..p }, 2).is_err()
    );
    assert_eq!(palw_x3_material_term_with_retention_v1(1_000, &p, None), Ok(1_000), "old ruleset: the term unchanged");
    assert_eq!(
        palw_x3_material_term_with_retention_v1(1_000, &p, Some(&ext)),
        Ok(1_000 + ext.extra_daa * p.retention_cost_per_daa_sompi)
    );

    // The fold's one claim writer raises it at creation; the paid consumer's plan reads the obligation and holds material to hard.
    let builder = include_str!("../palw_state_v2/transition_builder.rs");
    assert!(builder.contains("Some(mut claim) if !self.state.claims.contains_key(&key) => {"));
    assert!(builder.contains("crate::palw_ruling_fences_v1::palw_claim_x3_retention_daa_v1(self.params, &claim)"));
    let consumer = include_str!("../palw_bond_budget_fold_v1/consumer.rs");
    assert!(
        consumer
            .contains("crate::palw_ruling_fences_v1::palw_claim_retention_obligation_v1(self.params, claim) < terms.material_until")
    );
    assert!(consumer.contains("X3: a paid consumer's material must be held through the claim's hard deadline"));
}

/// REQ-X3-2: `effective = min(proposed, hard)` — in the helper and in DL-1's licensed Final row.
#[test]
fn req_x3_2_the_effective_deadline_is_min_of_proposed_and_hard() {
    assert_eq!(palw_claim_effective_deadline_v1(500, None), 500, "no hard deadline: the proposed one, unchanged");
    assert_eq!(palw_claim_effective_deadline_v1(500, Some(400)), 400);
    assert_eq!(palw_claim_effective_deadline_v1(300, Some(400)), 300);
    assert_eq!(palw_claim_effective_deadline_v1(400, Some(400)), 400);

    // DL-1 itself: the same licensed claim under the old ruleset and under X3.
    let state = PalwChainStateV2::genesis();
    let claim_id = Hash64::from_u64_word(0xC1A1);
    let mut c = claim(FENCE, 1_000_000);
    let p = policy();
    let hard = FENCE + p.prepare_daa + p.observe_daa + p.resolve_daa;
    // Licensed late, so the proposed deadline (licence + challenge window) runs past the hard deadline.
    c.phase = PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: hard - 1 };
    let old = palw_rcore_deadline_v1(&state, &shipped_sp(), &claim_id, &c, None).expect("DL-1 reads the record");
    let new = palw_rcore_deadline_v1(&state, &armed_sp(), &claim_id, &c, None).expect("DL-1 reads the record");
    assert_eq!(new, old.map(|proposed| proposed.min(hard)), "X3 caps the proposed deadline at hard, and nothing else");
    // A claim accepted below the fence keeps the old row exactly.
    let mut before = c.clone();
    before.accepted_daa = FENCE - 1;
    assert_eq!(
        palw_rcore_deadline_v1(&state, &armed_sp(), &claim_id, &before, None).unwrap(),
        palw_rcore_deadline_v1(&state, &shipped_sp(), &claim_id, &before, None).unwrap()
    );
}

/// REQ-X3-3: no extension and no re-grant for a claim accepted past the fence; the live rules (held-decoy re-arm, DA pause credit,
/// V6 horizon credit, staged-court `hard.max(new)`, model-window retention extension) stay the OLD rules below it. Each guard asks
/// [`palw_claim_x3_governs_v1`]; the source pins below hold every guard in its place.
#[test]
fn req_x3_3_no_extension_or_regrant_for_a_new_claim_and_the_old_rules_stay_for_old_claims() {
    let sp = armed_sp();
    assert!(palw_claim_x3_governs_v1(&sp, &claim(FENCE, 1_000_000)));
    assert!(!palw_claim_x3_governs_v1(&sp, &claim(FENCE - 1, 1_000_000)), "an old claim keeps the old (extending) rules");
    let pins: [(&str, &str, &[&str]); 5] = [
        (
            "palw_state_v2.rs (held-decoy re-arm)",
            include_str!("../palw_state_v2.rs"),
            &["&& !crate::palw_ruling_fences_v1::palw_claim_x3_governs_v1(builder.params, claim) =>"],
        ),
        (
            "transition_builder.rs (DA pause credit, V6 credit)",
            include_str!("../palw_state_v2/transition_builder.rs"),
            &[
                "if credit_pause && !crate::palw_ruling_fences_v1::palw_claim_x3_governs_v1(self.params, &claim) {",
                // `credit_verify_pause_v6` returns its writer's Result since the RFC06 typed-capacity writers (b496ee2ba).
                "if crate::palw_ruling_fences_v1::palw_claim_x3_governs_v1(self.params, claim) {\n            return Ok(());\n        }",
            ],
        ),
        (
            "palw_staged_court_fold_v1.rs (paid stage extension)",
            include_str!("../palw_staged_court_fold_v1.rs"),
            &["X3: a paid stage cannot extend a claim's deadlines fixed at acceptance"],
        ),
        (
            "da_sessions.rs (model-window retention extension)",
            include_str!("../palw_state_v2/da_sessions.rs"),
            &["X3: the model court window outruns the claim's retention fixed at acceptance"],
        ),
        (
            "palw_state_v2.rs (DL-1 min)",
            include_str!("../palw_state_v2.rs"),
            // DL-1 reads X3's cap through the X3 × clock-v2 combined rule (G14 r6 item 3), which calls
            // `palw_claim_hard_deadline_of_v1` itself; the pin follows the call site (it was a stale string).
            &["crate::palw_court_clock_v2::palw_claim_x3_cap_hard_v2(params, claim)"],
        ),
    ];
    for (site, src, needles) in pins {
        for needle in needles {
            assert!(src.contains(needle), "{site}: the X3 guard is in place ({needle})");
        }
    }
    // The staged court guard sits on BOTH paid-stage extensions (prefix and tokenizer).
    assert_eq!(
        include_str!("../palw_staged_court_fold_v1.rs").matches("X3: a paid stage cannot extend").count(),
        2,
        "both staged extensions are guarded"
    );
}

/// REQ-X3-4 (and REQ-T12-3): a pursuit starts only if `start <= open_cutoff` and `start + B_remaining <= hard`; the boundaries are
/// `<=` admitted, `>` refused. `B_remaining` is the policy's five named terms plus the network's disclose and court windows.
#[test]
fn req_x3_4_a_pursuit_starts_only_inside_its_budget() {
    let sp = armed_sp();
    let p = policy();
    let budget = palw_claim_pursuit_budget_v1(&sp, &p);
    assert_eq!(
        budget,
        p.cold_fetch_daa
            + p.check_daa
            + p.localize_daa
            + palw_da_disclose_window_daa_v1(&sp)
            + sp.window_court()
            + p.carrier_daa
            + p.reorg_slack_daa
    );
    // A claim whose resolve span leaves room: the binding bound is the open cutoff or `hard − B`, whichever is earlier.
    let c = claim(FENCE, u64::MAX / 4);
    let d = palw_claim_deadlines_v1(&sp, &c).unwrap();
    let last = d.open_cutoff_daa.min(d.hard_deadline_daa.saturating_sub(budget));
    if d.hard_deadline_daa >= budget && last >= c.accepted_daa {
        assert_eq!(palw_claim_pursuit_may_start_v1(&sp, &c, last), Ok(()), "admitted at the boundary");
    }
    assert!(palw_claim_pursuit_may_start_v1(&sp, &c, last + 1).is_err(), "refused one past it");
    assert!(palw_claim_pursuit_may_start_v1(&sp, &c, d.open_cutoff_daa + 1).is_err(), "never after the open cutoff");
    assert!(palw_claim_pursuit_may_start_v1(&sp, &c, d.hard_deadline_daa).is_err(), "never at the hard deadline");
    // Old claims: no start budget (the old ruleset).
    assert_eq!(palw_claim_pursuit_may_start_v1(&sp, &claim(FENCE - 1, FENCE + 10), u64::MAX), Ok(()));
    // The reservation (tag 154) and the named-leaf dissection both ask it.
    assert!(
        include_str!("../palw_legacy_public_filer_fold_v1.rs")
            .contains("crate::palw_ruling_fences_v1::palw_claim_pursuit_may_start_v1(builder.params, &claim, now).map_err(refused)?;")
    );
    assert!(
        include_str!("../palw_state_v2/da_sessions.rs")
            .contains("crate::palw_ruling_fences_v1::palw_claim_pursuit_may_start_v1(builder.params, claim, ctx.daa_score)")
    );
}

/// REQ-X3-5: reaching the deadline alone never Finals an unresolved claim — DL-1 gives an X3 licensed claim with an open DA session
/// no deadline (so the sweep cannot Final it), the session's open disarms and its last close re-derives it, and the hard-deadline
/// lapse keeps a reservation whose session is still open. Contrast: LG14-A's
/// `lg14a_a_reservation_holds_final_to_the_hard_deadline_then_lapses_…` (the OLD rule, kept for old claims).
#[test]
fn req_x3_5_reaching_the_deadline_alone_never_finals_an_unresolved_claim() {
    let dl1 = include_str!("../palw_state_v2.rs");
    let row = dl1.find("**X3 (REQ-X3-5, `palw_claim_hard_deadline_v1`)").expect("DL-1's X3 hold row");
    let licensed = dl1[row..].find("PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => {").expect("the licensed row follows");
    let hold = &dl1[row..row + licensed];
    assert!(hold.contains("palw_claim_x3_governs_v1(params, claim)") && hold.contains("record.open_sessions() > 0"), "{hold}");
    assert!(hold.contains("None"), "the hold gives no deadline");
    let sessions = include_str!("../palw_state_v2/da_sessions.rs");
    assert!(
        sessions.contains("X3 (REQ-X3-5): on a licensed claim accepted past `palw_claim_hard_deadline_v1` any open session holds")
    );
    let builder = include_str!("../palw_state_v2/transition_builder.rs");
    assert!(builder.contains("X3 (REQ-X3-5): any session held this claim's Final (DL-1's X3 row); the last close re-derives it."));
    let sweep = include_str!("../palw_legacy_public_filer_fold_v1.rs");
    assert!(sweep.contains("partition(|reserver| builder.state.da_sessions.contains_key(&(claim_id, *reserver)))"));
    assert!(sweep.contains("skip_past = Some((deadline, claim_id));"));
}

// ---- N4 ------------------------------------------------------------------------------------------------------------------------

/// REQ-N4-1: past the fence a free-prompt claim whose required evidence cannot be public — PanelDa, FP Job V5 — is refused at the
/// claim gate (no new reward or Final weight); a public-evidence job is not.
#[test]
fn req_n4_1_a_nonpublic_evidence_route_earns_no_new_reward_or_weight() {
    let sp = armed_sp();
    let public = commitment(job(PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_V4_VERSION));
    assert_eq!(palw_fp_job_evidence_public_v1(&public.job), Ok(()));
    assert_eq!(palw_public_evidence_reward_refusal_v1(&sp, FENCE, Some(&public)), None, "public evidence: the old path");

    let panel_da = commitment(job(PALW_FP_PRIVACY_PANEL_DA, PALW_FP_V4_VERSION));
    assert!(palw_public_evidence_reward_refusal_v1(&sp, FENCE, Some(&panel_da)).is_some_and(|why| why.contains("PanelDa")));

    let v5 = commitment(job(PALW_FP_PRIVACY_PUBLIC_DA, crate::palw_fp_job_v5::PALW_FP_V5_VERSION));
    assert!(palw_public_evidence_reward_refusal_v1(&sp, FENCE, Some(&v5)).is_some_and(|why| why.contains("V5")));
    let mut v5_tail = job(PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_V4_VERSION);
    v5_tail.tail = Some(PalwFpJobTailV1::V5(crate::palw_fp_job_v5::PalwFpV5TailV1 {
        images: vec![crate::palw_gen_class_v1::PalwGenImageInputRefV1 { input_root: Hash64::from_u64_word(9), h: 4, w: 4 }],
        source: None,
    }));
    assert!(palw_fp_job_evidence_public_v1(&v5_tail).is_err(), "an image tail is non-public whatever the version word says");

    assert!(palw_public_evidence_reward_refusal_v1(&sp, FENCE, None).is_some(), "nothing shows the evidence public");
    // The fold asks it at the claim gate with the reward gate's error and its own code.
    let objects = include_str!("../palw_state_v2/objects.rs");
    assert!(objects.contains("crate::palw_ruling_fences_v1::palw_public_evidence_reward_refusal_v1(\n                builder.params,\n                ctx.daa_score,\n                public_job.as_deref(),"));
    assert!(objects.contains("code: crate::palw_ruling_fences_v1::PALW_N4_NONPUBLIC_EVIDENCE_CODE_V1,"));
}

/// REQ-N4-2: no retroactivity — a claim accepted below the fence (the gate is asked only at ACCEPTANCE, with the accepting block's
/// DAA) keeps its old ruleset to its end; no other N4 hook exists on the reward, payout or Final path.
#[test]
fn req_n4_2_claims_accepted_before_the_fence_keep_the_old_ruleset() {
    let sp = armed_sp();
    let panel_da = commitment(job(PALW_FP_PRIVACY_PANEL_DA, PALW_FP_V4_VERSION));
    let v5 = commitment(job(PALW_FP_PRIVACY_PUBLIC_DA, crate::palw_fp_job_v5::PALW_FP_V5_VERSION));
    for accepted in [0, FENCE - 1] {
        assert_eq!(palw_public_evidence_reward_refusal_v1(&sp, accepted, Some(&panel_da)), None);
        assert_eq!(palw_public_evidence_reward_refusal_v1(&sp, accepted, Some(&v5)), None);
        assert_eq!(palw_public_evidence_reward_refusal_v1(&sp, accepted, None), None);
    }
    let mut hooks = 0;
    for src in [
        include_str!("../palw_state_v2.rs"),
        include_str!("../palw_state_v2/objects.rs"),
        include_str!("../palw_state_v2/transition_builder.rs"),
        include_str!("../palw_state_v2/fold_read.rs"),
    ] {
        hooks += src.matches("palw_public_evidence_reward_refusal_v1(").count();
    }
    assert_eq!(hooks, 1, "N4 is asked once, at acceptance — never at licence, Final or payout");
}

/// REQ-N4-3: a weightless tensor claim (tag 87) — PanelDa or image inputs — earns nothing today, so N4 refuses nothing; the tensor
/// fold asks N4 at the one site that writes the record, gated on the record earning reward or weight, so the fence that later
/// grants tensor claims weight cannot grant it to a non-public route past N4.
#[test]
fn req_n4_3_a_weightless_tensor_claim_is_guarded_where_weight_would_be_granted() {
    use crate::palw_gen_job_v1::PalwGenAcceptedJobV1;
    let job = |public_da: bool, images: usize| PalwGenAcceptedJobV1 {
        profile: crate::palw_gen_v1::PalwGenProfileV1::Embedding,
        item_index: 0,
        steps: 0,
        scalars: vec![],
        prompt_tokens: 4,
        prompt_hash: Hash64::from_u64_word(0x77),
        negative_tokens: 0,
        negative_hash: Hash64::default(),
        images: (0..images)
            .map(|i| crate::palw_gen_class_v1::PalwGenImageInputRefV1 { input_root: Hash64::from_u64_word(i as u64), h: 4, w: 4 })
            .collect(),
        public_da,
    };
    let sp = armed_sp();
    assert_eq!(palw_gen_job_evidence_public_v1(&job(true, 0)), Ok(()));
    assert!(palw_public_evidence_tensor_refusal_v1(&sp, FENCE, &job(false, 0)).is_some_and(|why| why.contains("PanelDa")));
    assert!(palw_public_evidence_tensor_refusal_v1(&sp, FENCE, &job(true, 1)).is_some_and(|why| why.contains("image")));
    assert_eq!(palw_public_evidence_tensor_refusal_v1(&sp, FENCE, &job(true, 0)), None);
    assert_eq!(palw_public_evidence_tensor_refusal_v1(&sp, FENCE - 1, &job(false, 1)), None, "below the fence: the old ruleset");
    assert_eq!(palw_public_evidence_tensor_refusal_v1(&shipped_sp(), u64::MAX, &job(false, 1)), None);

    // A tensor claim's record, as the fold writes it, earns nothing — so the guard refuses nothing today.
    let mut weightless = claim(FENCE, 1_000_000);
    weightless.source = crate::palw_state_v2::PalwClaimSourceV2::FreePrompt { quanta: 0, spent: Default::default() };
    assert!(!palw_claim_earns_reward_or_weight_v1(&weightless));
    let earns: [fn(&mut PalwClaimStateV2); 5] = [
        |c: &mut PalwClaimStateV2| c.pwu = 1,
        |c: &mut PalwClaimStateV2| c.escrowed_reward = 1,
        |c: &mut PalwClaimStateV2| c.rights_reserved = 1,
        |c: &mut PalwClaimStateV2| c.immature_contribution = 1,
        |c: &mut PalwClaimStateV2| {
            c.source = crate::palw_state_v2::PalwClaimSourceV2::FreePrompt { quanta: 1, spent: Default::default() }
        },
    ];
    for earn in earns {
        let mut weighted = weightless.clone();
        earn(&mut weighted);
        assert!(palw_claim_earns_reward_or_weight_v1(&weighted), "{weighted:?}");
    }
    // The tensor fold asks it where the record is written, behind the earn test.
    let fold = include_str!("../palw_gen_claim_fold_v1.rs");
    assert!(fold.contains("if crate::palw_ruling_fences_v1::palw_claim_earns_reward_or_weight_v1(&claim)"));
    assert!(fold.contains("crate::palw_ruling_fences_v1::palw_public_evidence_tensor_refusal_v1(builder.params, daa, &accepted)"));
    assert!(
        fold.contains("pwu: 0,") && fold.contains("escrowed_reward: 0,") && fold.contains("quanta: 0"),
        "tag 87 is weightless today"
    );
}

// ---- D2 ------------------------------------------------------------------------------------------------------------------------

/// REQ-T6-3, restricted: past the fence the four ARMED t12 routes that compel model bytes are not admissible for a claim accepted
/// at/after it; a claim accepted before keeps them; every other unit is untouched.
#[test]
fn req_t6_3_restricted_the_armed_model_byte_routes_close_for_new_claims_only() {
    // The restricted set is exactly the inventory's armed model-byte units.
    let armed: Vec<K> = PALW_D2_RESTRICTED_UNITS_V1
        .iter()
        .copied()
        .filter(|k| palw_court_demand_allowed_v1(*k).is_err() && palw_court_unit_scope_v1(*k).route.armed_on_testnet_12())
        .collect();
    assert_eq!(armed, PALW_D2_RESTRICTED_UNITS_V1.to_vec());

    let sp = armed_sp();
    let new = claim(FENCE, 1_000_000);
    let old = claim(FENCE - 1, 1_000_000);
    for kind in PALW_D2_RESTRICTED_UNITS_V1 {
        assert!(
            matches!(palw_court_model_scope_admits_v1(&sp, &new, kind), Err(CourtScopeRefusalV1::ModelBytes(m)) if m.is_model_bytes()),
            "{kind:?} is closed for a new claim"
        );
        assert_eq!(palw_court_model_scope_admits_v1(&sp, &old, kind), Ok(()), "{kind:?} stays open for an old claim");
    }
    for kind in [K::RCoreEvent, K::HeldStepRange, K::TirStepMaterial, K::AccuserArtifactOpenings, K::KernelParamOpening] {
        assert_eq!(palw_court_model_scope_admits_v1(&sp, &new, kind), Ok(()), "{kind:?} is not one of the four");
    }
    // Readiness: per class — a class registered at/after the fence takes no proof past it; one registered before keeps readiness.
    assert!(palw_court_model_scope_admits_readiness_v1(&sp, FENCE, Some(FENCE)).is_err());
    assert_eq!(palw_court_model_scope_admits_readiness_v1(&sp, FENCE, Some(FENCE - 1)), Ok(()));
    assert_eq!(palw_court_model_scope_admits_readiness_v1(&sp, FENCE - 1, Some(FENCE)), Ok(()), "below the fence");
    assert_eq!(palw_court_model_scope_admits_readiness_v1(&sp, FENCE, None), Ok(()), "unknown class: its own refusals answer");

    // The fold asks it at the held StepLeaf demand, at every named-leaf dissection (where the producer-opening units are replaced by
    // operands the opening publishes, REQ-T6-4 — `palw_court_model_scope_routes_v1`'s tests), and at the three readiness arms.
    let sessions = include_str!("../palw_state_v2/da_sessions.rs");
    assert!(sessions.contains("crate::palw_court_scope_v1::CourtUnitKindV1::HeldStepLeaf,"));
    assert!(sessions.contains("if crate::palw_ruling_fences_v1::palw_court_model_scope_governs_claim_v1(builder.params, claim) {"));
    assert!(sessions.contains("crate::palw_court_model_scope_routes_v1::palw_d2_named_leaf_operands_admit_v1("));
    assert_eq!(
        include_str!("../palw_state_v2/objects.rs").matches("check_readiness_model_scope_v1(builder, ctx, class_id)?;").count(),
        3
    );
}

// ---- the arm-readiness records (2026-10-18, lane d2-x3-finish): D2 and X3 refused without a record, admitted only by one ----------

/// A well-formed arm-readiness record for `fence` on the network `genesis` at `daa` (distinct non-zero digests, one per requirement).
fn arm_record(
    fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1,
    genesis: Hash64,
    daa: u64,
) -> crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
    let n = fence.requirements().len();
    let evidence: Vec<[u8; 32]> = (0..n).map(|i| [i as u8 + 1; 32]).collect();
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence,
        genesis,
        activation_daa: daa,
        source_commit: [7u8; 20],
        evidence: Box::leak(evidence.into_boxed_slice()),
        review: [0xEE; 32],
    }
}

#[test]
fn d2_and_x3_are_refused_without_a_record_and_admitted_exactly_at_the_records_network_and_height() {
    use crate::palw_g14_gates_v1::PalwG14GatedFenceV1 as G;
    let mut p = t12();
    p.palw_legacy_held_da_v2 = Some(ForkActivation::new(FENCE - 1));
    p.palw_court_model_scope_v1 = Some(ForkActivation::new(FENCE));
    p.palw_claim_hard_deadline_v1 = Some(composed_fence());
    p.sync_palw_2026_10_12_ruling_fences_v1();
    let genesis = p.genesis.hash;

    // Refused with no record, and the refusal names the fence.
    let none = p.validate_palw_2026_10_12_ruling_fences_in_v1(&[]).expect_err("no record: refused");
    assert!(format!("{none:?}").contains("palw_court_model_scope_v1"), "{none:?}");
    // Each fence on its own, as a record for the other one does not admit it.
    let d2_only = [arm_record(G::CourtModelScopeV1, genesis, FENCE)];
    let x3_refusal = p.validate_palw_2026_10_12_ruling_fences_in_v1(&d2_only).expect_err("the X3 record is missing");
    assert!(format!("{x3_refusal:?}").contains("palw_claim_hard_deadline_v1"), "{x3_refusal:?}");
    // Both records, at exactly the fence's height on this network: admitted.
    let both = [arm_record(G::CourtModelScopeV1, genesis, FENCE), arm_record(G::ClaimHardDeadlineV1, genesis, FENCE)];
    p.validate_palw_2026_10_12_ruling_fences_in_v1(&both).expect("both records name this network and height");
    // A record one DAA off, or for another network, does not admit it.
    let off_by_one = [arm_record(G::CourtModelScopeV1, genesis, FENCE + 1), arm_record(G::ClaimHardDeadlineV1, genesis, FENCE)];
    assert!(p.validate_palw_2026_10_12_ruling_fences_in_v1(&off_by_one).is_err(), "one DAA off: refused");
    let other_net = Hash64::from_u64_word(0x9999);
    let other = [arm_record(G::CourtModelScopeV1, other_net, FENCE), arm_record(G::ClaimHardDeadlineV1, other_net, FENCE)];
    assert!(p.validate_palw_2026_10_12_ruling_fences_in_v1(&other).is_err(), "another network: refused");
    // The shipped list stays empty, so the binary's own validation still refuses both on testnet-12.
    assert!(
        crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORDS_V1
            .iter()
            .all(|r| r.activation_daa == 17_400 && r.genesis == crate::config::genesis::PALW_T12_GENESIS.hash)
    );
    assert!(p.validate_palw_2026_10_12_ruling_fences_v1().is_err());
    // A record of the wrong form (zero review digest) is refused even when it names the right network and height.
    let mut bad = both;
    bad[1].review = [0; 32];
    assert!(p.validate_palw_2026_10_12_ruling_fences_in_v1(&bad).is_err(), "a record must be well formed");
}

#[test]
fn x3_refuses_an_unmeasured_policy_term_even_with_a_record() {
    use crate::palw_g14_gates_v1::PalwG14GatedFenceV1 as G;
    let mut p = t12();
    // The candidate derived from the horizons leaves six terms unmeasured (0): refused whatever records exist.
    let candidate = PalwClaimHardDeadlineFenceV1::candidate_from_horizons_v1(ForkActivation::new(FENCE), &shipped_sp());
    assert_eq!(palw_x3_unmeasured_terms_v1(&candidate.policy).len(), 6);
    p.palw_claim_hard_deadline_v1 = Some(candidate);
    p.sync_palw_2026_10_12_ruling_fences_v1();
    let genesis = p.genesis.hash;
    let records = [arm_record(G::ClaimHardDeadlineV1, genesis, FENCE)];
    let why = p.validate_palw_2026_10_12_ruling_fences_in_v1(&records).expect_err("unmeasured terms");
    assert!(format!("{why:?}").contains("unmeasured"), "{why:?}");
    // With every term measured (the probe's nonzero values stand in for measurements in this fixture), the record admits it.
    let mut measured = t12();
    measured.palw_claim_hard_deadline_v1 = Some(composed_fence());
    measured.sync_palw_2026_10_12_ruling_fences_v1();
    measured
        .validate_palw_2026_10_12_ruling_fences_in_v1(&[arm_record(G::ClaimHardDeadlineV1, genesis, FENCE)])
        .expect("measured and recorded");
}

#[test]
fn x3_spans_are_the_composition_of_the_windows_read_at_activation() {
    let sp = shipped_sp();
    let activation = ForkActivation::new(FENCE);
    let fence = composed_fence();
    assert_eq!(palw_x3_composition_refusal_v1(&sp, activation, &fence.policy), None, "the composed policy agrees with the windows");
    // The probe's spans are not the windows' composition: refused at activation.
    let probe = PalwClaimHardDeadlineFenceV1::unapproved_probe_at(activation).policy;
    assert!(palw_x3_composition_refusal_v1(&sp, activation, &probe).is_some());
    // Δ is part of T_resolve: a policy whose resolve omits Δ is refused.
    let no_delta =
        PalwClaimHardDeadlinePolicyV1 { resolve_daa: fence.policy.resolve_daa - fence.policy.cold_fetch_daa, ..fence.policy };
    assert!(palw_x3_composition_refusal_v1(&sp, activation, &no_delta).is_some(), "resolve must include the five B_* terms");
    // The material term with a positive extension and an unmeasured c_retain is refused, never priced at zero.
    let unmeasured = PalwClaimHardDeadlinePolicyV1 { retention_cost_per_daa_sompi: 0, ..fence.policy };
    assert!(
        palw_x3_material_term_with_retention_v1(10, &unmeasured, Some(&palw_x3_retention_extension_at_v1(&unmeasured, 100, 0)))
            .is_err()
    );
    let priced = palw_x3_retention_extension_at_v1(&fence.policy, 100, 0);
    assert_eq!(
        palw_x3_material_term_with_retention_v1(10, &fence.policy, Some(&priced)),
        Ok(10 + priced.extra_daa * fence.policy.retention_cost_per_daa_sompi)
    );
}
