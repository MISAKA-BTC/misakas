//! **ADR-0160 stage 2 — the audit door (F-Q) and the issuance slots (F-S), through testnet-12's own
//! fold** (rcore/cap-s1; the user's staged-safety plan, stage 2's gate: "mass claims keep liability,
//! rate and outstanding controlled; detection must be an on-chain observable (audit receipts)").
//!
//! Every test runs the stage-1 capacity list armed at [`H`] with a CREDITED F-L step (ρ and a credit
//! reaching `q_seat`, so a claim's escrow slot is cut below its reward) and F-Q / F-S armed at `H` too,
//! on the stage-1 fixture's [`Sim`] (every block's delta re-applies and reverts, its carriage reloads
//! under its root):
//!
//! * **Q** — a credited claim licenses and then WAITS: no `Final` deadline until `k_aud` distinct pool
//!   members (operator cards neither producing it nor seated on its panel) posted an
//!   `AuditReceiptBatchV1` (tag 60) whose root is the claim's; `k_aud` is 1 below ρ 250 and 2 from it; a
//!   receipt by a seat or the producer is skipped, by a non-operator or with a wrong root refuses the
//!   block; an uncredited claim is not gated; a conviction of an audited claim excludes its auditor for
//!   good; an unaudited claim is never voided or charged while it waits.
//! * **S** — the slot is held from acceptance to a counted licence (or a void's `h_obl`), the bucket
//!   bursts `B` and refills `r` a DAA, and `N_out` caps the outstanding claims; at ρ 100 a 13,000 MSK
//!   bond holds at most 200 outstanding and issues 10 a DAA (its burst: one DAA's refill).
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_capacity_stage2_q_s -- --nocapture`

#[path = "capacity_stage1_common.rs"]
mod stage1;
use stage1::*;

use kaspa_consensus_core::config::params::PALW_T12_CAPACITY_FENCES_V1;
use kaspa_consensus_core::palw_aggregate_liability_v1::{PalwCapacityLiabilityV1, PalwCapacityStepV1};
use kaspa_consensus_core::palw_audit_door_v1::{
    PalwAuditEntryV1, palw_audit_pool_of_claim_v1, palw_capacity_audit_backlog_v1, palw_capacity_claim_credited_v1,
};
use kaspa_consensus_core::palw_issuance_slots_v1::palw_issuance_read_at_v1;

/// testnet-12 with stage 1's and stage 2's capacity fences at [`H`] (every later stage's dormant), F-L's
/// one step `(ρ, q)`.
fn stage2_params(class: Class, rho: u32, q: u16) -> Params {
    let mut p = params_for(class, false);
    for fence in PALW_T12_CAPACITY_FENCES_V1.iter().filter(|f| STAGE1_FENCES.contains(&f.name) || STAGE2_FENCES.contains(&f.name)) {
        (fence.set)(&mut p, Some(ForkActivation::new(H)));
    }
    p.palw_capacity_aggregate_liability = Some(PalwCapacityLiabilityV1 {
        activation: ForkActivation::new(H),
        steps: vec![PalwCapacityStepV1 { from_daa: H, rho, q_credit_permille: q }],
    });
    p.sync_palw_capacity_liability();
    p.validate_palw_v2().unwrap_or_else(|e| panic!("stage 2 at ρ {rho}, q {q} validates: {e:?}"));
    p
}

/// A receipt batch by `auditor` for `claims`, each with the root `root_of` gives (the fold never reads
/// the signature: the acceptance layer's).
fn receipt(auditor: PalwBondKeyV2, entries: &[(Hash64, Hash64)]) -> PalwConsensusObjectV2 {
    let mut entries: Vec<PalwAuditEntryV1> = entries.iter().map(|(claim_id, root)| PalwAuditEntryV1 { claim_id: *claim_id, reproduced_root: *root }).collect();
    entries.sort();
    PalwConsensusObjectV2::AuditReceiptBatchV1 { auditor, entries, signature: vec![] }
}

fn root(sim: &Sim, id: &Hash64) -> Hash64 {
    sim.c.claim(id).execution_root
}

fn phase(sim: &Sim, id: &Hash64) -> Option<PalwClaimPhaseV2> {
    sim.c.s.claim(id).map(|c| c.phase.clone())
}

/// A claim of bond 90 bound and licensed on the class's honest panel; its id and its licence DAA.
fn licensed_claim(sim: &mut Sim, seed: u64) -> (Hash64, u64) {
    let id = sim.claim(90, seed).expect("admitted");
    let seats = sim.seats();
    let bound = sim.bind(id, &seats);
    sim.license(id, &seats, bound);
    (id, sim.c.daa)
}

/// **Q: a credited claim reaches `Final` only through its audit** (k_aud = 1 at ρ 10). It licenses,
/// passes its licensed `Final` floor and waits (no deadline, not voided, nothing charged); a receipt by a
/// pool member lands on chain (`audit_receipts` holds it — the observable) and the claim is `Final` the
/// next block, with its vesting row.
#[test]
fn q_a_credited_claim_waits_for_its_audit_then_finals() {
    let mut sim = Sim::new(stage2_params(Class::Floor, 10, 250), Class::Floor, &[(90, 100_000)]);
    let (id, licensed_at) = licensed_claim(&mut sim, 0xA0D1);
    let claim = sim.c.claim(&id);
    assert!(palw_capacity_claim_credited_v1(&sim.c.sp, &claim), "the premise: credited (m_c < E past F-Q)");
    assert_eq!(sim.c.s.deadline_of(&id), None, "an unaudited credited claim owes no deadline");
    let collateral = sim.c.s.bond(&bond_key(90)).unwrap().collateral;
    sim.block(licensed_at + 400, vec![], None);
    assert!(matches!(phase(&sim, &id), Some(PalwClaimPhaseV2::ReceiptLicensed { .. })), "it waits past its floor: {:?}", phase(&sim, &id));
    assert_eq!(sim.c.s.bond(&bond_key(90)).unwrap().collateral, collateral, "never charged for waiting");
    assert!(sim.c.s.vesting_row(&id).is_none(), "and never paid");
    assert_eq!(palw_capacity_audit_backlog_v1(&sim.c.s, &sim.c.sp), 1, "it is the backlog");
    let pool = palw_audit_pool_of_claim_v1(&sim.c.s, &sim.c.sp, &id, &claim);
    assert_eq!(pool.len(), 3, "t12's pool: the eight cards less the five seats");
    let auditor = pool[0];
    let r = root(&sim, &id);
    sim.step(vec![receipt(auditor, &[(id, r)])]);
    let audited_at = sim.c.daa;
    let status = sim.c.s.audit_status_of_v1(&id).expect("the receipt is on chain");
    assert_eq!(status.receipts, vec![(auditor, audited_at)], "the audit is an on-chain observable");
    assert_eq!(sim.c.s.deadline_of(&id), Some(audited_at), "audited: its deadline is max(floor, audit_daa)");
    sim.step(vec![]);
    assert!(matches!(phase(&sim, &id), Some(PalwClaimPhaseV2::Final { .. })), "Final after its audit: {:?}", phase(&sim, &id));
    assert!(sim.c.s.vesting_row(&id).is_some(), "and its row vests");
    assert_eq!(palw_capacity_audit_backlog_v1(&sim.c.s, &sim.c.sp), 0);
    println!("Q: credited claim licensed at {licensed_at}, waited to {audited_at}, audited by {auditor:?}, Final at {}", sim.c.daa);
}

/// **Q: who may receipt, and what refuses the block** — a seat of the claim's panel and the claim's own
/// producer are skipped (the state does not move); a bond that is not an operator, and a root that is
/// not the claim's, refuse the block; the same auditor twice counts once.
#[test]
fn q_receipts_outside_the_pool_are_skipped_and_invalid_ones_refuse_the_block() {
    let mut sim = Sim::new(stage2_params(Class::Floor, 10, 250), Class::Floor, &[(90, 100_000)]);
    let (id, _) = licensed_claim(&mut sim, 0xA0D2);
    let claim = sim.c.claim(&id);
    let r = root(&sim, &id);
    let seat = sim.c.s.panel(&id).unwrap().seats[0].bond;
    sim.step(vec![receipt(seat, &[(id, r)])]);
    assert!(sim.c.s.audit_status_of_v1(&id).is_none(), "a seat of its panel is not its auditor: skipped");
    let daa = sim.c.daa + 1;
    let refused = sim.try_block(daa, vec![receipt(bond_key(CHALLENGER), &[(id, r)])]);
    assert!(refused.as_ref().is_err_and(|e| e.contains("not an operator bond")), "{refused:?}");
    let pool = palw_audit_pool_of_claim_v1(&sim.c.s, &sim.c.sp, &id, &claim);
    let refused = sim.try_block(daa, vec![receipt(pool[0], &[(id, h(0xBAD))])]);
    assert!(refused.as_ref().is_err_and(|e| e.contains("reproduced root")), "{refused:?}");
    sim.step(vec![receipt(pool[0], &[(id, r)])]);
    sim.step(vec![receipt(pool[0], &[(id, r)])]);
    assert_eq!(sim.c.s.audit_status_of_v1(&id).unwrap().receipts.len(), 1, "one auditor, one receipt");
}

/// **Q: `k_aud` = 2 from ρ 250** — one receipt leaves the claim waiting, the second distinct one audits it.
#[test]
fn q_two_receipts_from_rho_250() {
    let mut sim = Sim::new(stage2_params(Class::Floor, 250, 250), Class::Floor, &[(90, 1_000_000)]);
    let (id, licensed_at) = licensed_claim(&mut sim, 0xA0D3);
    let claim = sim.c.claim(&id);
    let pool = palw_audit_pool_of_claim_v1(&sim.c.s, &sim.c.sp, &id, &claim);
    let r = root(&sim, &id);
    sim.step(vec![receipt(pool[0], &[(id, r)])]);
    sim.block(licensed_at + 400, vec![], None);
    assert!(matches!(phase(&sim, &id), Some(PalwClaimPhaseV2::ReceiptLicensed { .. })), "one receipt is not two");
    sim.step(vec![receipt(pool[1], &[(id, r)])]);
    sim.step(vec![]);
    assert!(matches!(phase(&sim, &id), Some(PalwClaimPhaseV2::Final { .. })), "the second distinct receipt audits it");
}

/// **Q: an uncredited claim is not gated** — at a step whose credit does not reach `q_seat` the claim
/// finalizes at its floor with no receipt, F-Q armed or not.
#[test]
fn q_an_uncredited_claim_is_not_gated() {
    let mut sim = Sim::new(stage2_params(Class::Floor, 10, 0), Class::Floor, &[(90, 100_000)]);
    let (id, _) = licensed_claim(&mut sim, 0xA0D4);
    assert!(!palw_capacity_claim_credited_v1(&sim.c.sp, &sim.c.claim(&id)));
    let deadline = sim.c.s.deadline_of(&id).expect("today's licensed deadline");
    sim.block(deadline + 1, vec![], None);
    assert!(matches!(phase(&sim, &id), Some(PalwClaimPhaseV2::Final { .. })), "uncredited: Final at its floor");
}

/// **Q (§5.9 (g)): a conviction of an audited claim excludes its auditor** from every pool, for good: its
/// next batch refuses the block.
#[test]
fn q_a_conviction_of_an_audited_claim_excludes_its_auditor() {
    let mut sim = Sim::new(stage2_params(Class::Floor, 10, 250), Class::Floor, &[(90, 100_000)]);
    let (id, _) = licensed_claim(&mut sim, 0xA0D5);
    let (other, _) = licensed_claim(&mut sim, 0xA0D6);
    let claim = sim.c.claim(&id);
    let auditor = palw_audit_pool_of_claim_v1(&sim.c.s, &sim.c.sp, &id, &claim)[0];
    let r = root(&sim, &id);
    sim.step(vec![receipt(auditor, &[(id, r)])]);
    // Convicted before its `Final` (the receipt re-armed it at the next block: court first).
    sim.court_fraud(id);
    assert!(sim.c.s.auditor_excluded_v1(&auditor), "the auditor that passed a fraud is excluded");
    let daa = sim.c.daa + 1;
    let r2 = root(&sim, &other);
    let refused = sim.try_block(daa, vec![receipt(auditor, &[(other, r2)])]);
    assert!(refused.as_ref().is_err_and(|e| e.contains("excluded")), "{refused:?}");
}

/// **S: `N_out` caps the outstanding claims, a counted licence frees a slot** — a 1,000,000 MSK bond at
/// ρ 1 holds `u = 153` slots, fewer than its exposure ceiling's 156 floor claims, so lane S binds first;
/// licensing one frees one.
#[test]
fn s_the_outstanding_cap_binds_and_a_counted_licence_frees_a_slot() {
    let mut sim = Sim::new(stage2_params(Class::Floor, 1, 0), Class::Floor, &[(90, 1_000_000)]);
    let bond = bond_key(90);
    let collateral = sim.c.s.bond(&bond).unwrap().collateral;
    let mut ids = Vec::new();
    let base = sim.c.daa;
    for d in 1..=40u64 {
        for k in 0..12u64 {
            if let Some(id) = sim.block(base + d, vec![], Some((90, (d << 8) + k))) {
                ids.push(id);
            }
        }
    }
    let read = palw_issuance_read_at_v1(&sim.c.s, &sim.c.sp, &bond, collateral, sim.c.daa + 1).expect("F-S reads");
    assert_eq!((ids.len() as u64, read.outstanding, read.cap), (153, 153, 153), "u·ρ = 153 slots, below the ceiling's 156");
    assert!(sim.skips.keys().any(|k| k.contains("issuance is capped") && k.contains("Outstanding")), "lane S refused: {:?}", sim.skips);
    let seats = sim.seats();
    let bound = sim.bind(ids[0], &seats);
    sim.license(ids[0], &seats, bound);
    let read = palw_issuance_read_at_v1(&sim.c.s, &sim.c.sp, &bond, collateral, sim.c.daa + 1).unwrap();
    assert_eq!(read.outstanding, 152, "a counted licence frees its slot");
    assert!(sim.claim(90, 0x5E7E).is_some(), "and the bond takes one more");
}

/// **S: an unconvicted void holds its slot to `h_obl`** (S.5, with E-4's hold) — then frees it.
#[test]
fn s_a_void_holds_its_slot_for_h_obl() {
    let mut sim = Sim::new(stage2_params(Class::Floor, 1, 0), Class::Floor, &[(90, 13_000)]);
    let bond = bond_key(90);
    let collateral = sim.c.s.bond(&bond).unwrap().collateral;
    let id = sim.claim(90, 0x5102).expect("admitted");
    let voided_at = sim.c.claim(&id).accepted_daa + sim.c.sp.window_bind() + 1;
    sim.block(voided_at, vec![], None);
    assert!(matches!(phase(&sim, &id), Some(PalwClaimPhaseV2::Voided { .. })));
    let read = palw_issuance_read_at_v1(&sim.c.s, &sim.c.sp, &bond, collateral, sim.c.daa + 1).unwrap();
    assert_eq!(read.outstanding, 1, "an unconvicted void keeps its slot to voided + h_obl");
    sim.block(voided_at + sim.c.sp.window_receipt(), vec![], None);
    let read = palw_issuance_read_at_v1(&sim.c.s, &sim.c.sp, &bond, collateral, sim.c.daa + 1).unwrap();
    assert_eq!(read.outstanding, 0, "and frees it at the hold's end");
}

/// **Stage 2's gate — mass claims keep liability, rate and outstanding controlled.** A 13,000 MSK floor
/// bond at ρ 100 with the credit, twelve attempt blocks a DAA for 30 DAA: it never holds more than 200
/// slots, never issues more than its burst (10, one DAA's refill) in one DAA from a full bucket nor more than 10 a DAA after,
/// its provisional weight never exceeds `W_cap`, and no credited claim is paid (no vesting row) before its
/// audit — with none audited, none at all.
#[test]
fn stage2_gate_mass_claims_keep_liability_rate_and_outstanding_controlled() {
    let mut sim = Sim::new(stage2_params(Class::Floor, 100, 250), Class::Floor, &[(90, 13_000)]);
    let bond = bond_key(90);
    let collateral = sim.c.s.bond(&bond).unwrap().collateral;
    let w_cap = kaspa_consensus_core::palw_weight_cap_v1::palw_bond_weight_cap_v1(collateral);
    let mut ids = Vec::new();
    let base = sim.c.daa;
    let mut per_daa = Vec::new();
    for d in 1..=30u64 {
        let before = ids.len();
        for k in 0..12u64 {
            if let Some(id) = sim.block(base + d, vec![], Some((90, (d << 8) + k))) {
                ids.push(id);
            }
        }
        per_daa.push(ids.len() - before);
        let read = palw_issuance_read_at_v1(&sim.c.s, &sim.c.sp, &bond, collateral, sim.c.daa + 1).unwrap();
        assert!(read.outstanding <= read.cap, "S-I1 at DAA {}: {} > {}", base + d, read.outstanding, read.cap);
        let term = sim.c.s.capacity_weight_index().term(&bond, Some(collateral));
        assert!(term <= w_cap, "J-1");
    }
    assert!(per_daa[0] <= 10, "the first DAA: at most the burst (10), got {}", per_daa[0]);
    assert!(per_daa.iter().take(20).all(|n| *n == 10), "10 a DAA is reached (the burst is one DAA's refill): {per_daa:?}");
    assert!(per_daa.iter().skip(1).all(|n| *n <= 10), "then at most 10 a DAA: {per_daa:?}");
    assert!(ids.len() <= 200, "at most N_out = 200 outstanding");
    assert!(ids.iter().all(|id| sim.c.s.vesting_row(id).is_none()), "nothing paid before an audit");
    println!("stage 2 gate: 13k at ρ 100 issued {} claims in 30 DAA (per DAA {per_daa:?}), outstanding ≤ 200, J-1 held", ids.len());
}

/// **Q (§5.9 (h)): the audit duty's read** — a credited licensed claim awaiting its audit is offered to
/// each member of its pool (and to nobody else), with its pool ranked, `k_aud` and its job; once a member
/// receipts it, that member is offered it no more; once audited, nobody is.
#[test]
fn q_the_audit_duty_reads_its_candidates_off_the_chain() {
    use kaspa_consensus_core::palw_audit_door_v1::{palw_audit_on_turn_v1, palw_capacity_audit_candidates_v1};
    let mut sim = Sim::new(stage2_params(Class::Floor, 250, 250), Class::Floor, &[(90, 1_000_000)]);
    let (id, licensed_at) = licensed_claim(&mut sim, 0xA0D7);
    let claim = sim.c.claim(&id);
    let pool = palw_audit_pool_of_claim_v1(&sim.c.s, &sim.c.sp, &id, &claim);
    for member in &pool {
        let offered = palw_capacity_audit_candidates_v1(&sim.c.s, &sim.c.sp, member);
        assert_eq!(offered.len(), 1, "every pool member is offered the claim");
        assert_eq!((offered[0].claim_id, offered[0].licensed_daa, offered[0].k_aud), (id, licensed_at, 2));
        assert_eq!(offered[0].pool, pool);
        assert_eq!(offered[0].job.execution_root, claim.execution_root, "the replay's target");
    }
    let seat = sim.c.s.panel(&id).unwrap().seats[0].bond;
    assert!(palw_capacity_audit_candidates_v1(&sim.c.s, &sim.c.sp, &seat).is_empty(), "a seat is no auditor");
    assert!(palw_capacity_audit_candidates_v1(&sim.c.s, &sim.c.sp, &bond_key(90)).is_empty(), "nor the producer");
    let offered = palw_capacity_audit_candidates_v1(&sim.c.s, &sim.c.sp, &pool[0]);
    assert!(palw_audit_on_turn_v1(&offered[0], &pool[0], licensed_at) && palw_audit_on_turn_v1(&offered[0], &pool[1], licensed_at));
    assert!(!palw_audit_on_turn_v1(&offered[0], &pool[2], licensed_at), "k_aud 2: the third member waits a turn");
    let r = root(&sim, &id);
    sim.step(vec![receipt(pool[0], &[(id, r)])]);
    assert!(palw_capacity_audit_candidates_v1(&sim.c.s, &sim.c.sp, &pool[0]).is_empty(), "receipted: not offered again");
    assert_eq!(palw_capacity_audit_candidates_v1(&sim.c.s, &sim.c.sp, &pool[1]).len(), 1, "the others still are");
    sim.step(vec![receipt(pool[1], &[(id, r)])]);
    assert!(pool.iter().all(|m| palw_capacity_audit_candidates_v1(&sim.c.s, &sim.c.sp, m).is_empty()), "audited: offered to nobody");
}

/// **Q and S under reorg, restart and IBD** — a floor tape with the audit door and the slots armed at a
/// credited ρ 10: credited claims, their binds and licences, receipt batches, a refused over-cap attempt,
/// Finals through the audits; then every block reverted and re-applied, an IBD from the base, a restart at
/// every tip, and a fork with a different receipt order reorged to and back — root for root.
#[test]
fn q_s_reorg_restart_and_ibd_are_deterministic() {
    let mut c = Chain::new(stage2_params(Class::Floor, 10, 250));
    c.attribution = true;
    c.step(&[bond_obj(90, 13_000 * MSK), bond_obj(CHALLENGER, 400_000 * MSK)]);
    let mut t = Tape::new(c);
    let seats = t.c.floor_seats();
    let mut ids = Vec::new();
    for seed in 0..6u64 {
        let (env, key, id) = floor_attempt_of(&t.c, 90, 0x7A00 + seed);
        let anchor = floor_job_anchor(&t.c.p, bond_key(90), 0x10C0 + 0x7A00 + seed);
        let daa = t.c.daa + 1;
        t.block(daa, vec![], Some((env, key, anchor)), T12_BLOCK_SUBSIDY_SOMPI).expect("the block folds");
        if t.c.s.claim(&id).is_some() {
            ids.push(id);
        }
    }
    assert!(ids.len() >= 4, "one a DAA at ρ 10 (13k) admits at least four of six: {}", ids.len());
    for id in &ids {
        let bound = t.bind_to(*id, &seats);
        t.step(vec![PalwConsensusObjectV2::ReceiptLicensed { claim: *id, receipts: seats.iter().map(|(k, _)| valid(*id, *k, bound)).collect() }]);
    }
    let fork_at = t.len();
    let receipts = audit_receipts_for(&t.c.s, &t.c.sp, &ids);
    assert!(!receipts.is_empty(), "the credited claims wait for receipts");
    for batch in receipts.iter().cloned() {
        t.step(vec![batch]);
    }
    let last = ids.iter().filter_map(|id| t.c.s.deadline_of(id)).max().expect("audited claims owe deadlines");
    t.at(last + 1, vec![]);
    assert!(ids.iter().all(|id| matches!(t.c.s.claim(id).map(|c| c.phase.clone()), Some(PalwClaimPhaseV2::Final { .. }))), "Final through the audits");
    t.revert_to_base_and_reapply();
    t.ibd_from(t.base.clone());
    for j in 0..=t.len() {
        t.restart_at(j);
    }
    // A fork at the licence tip: the receipts in the other order.
    let mut fork = t.fork(fork_at);
    for batch in audit_receipts_for(&fork.c.s, &fork.c.sp, &ids).into_iter().rev() {
        fork.step(vec![batch]);
    }
    t.reorg_to(fork_at, &fork);
    println!("Q/S determinism: {} claims, {} blocks, fork at {fork_at} with the receipts reversed", ids.len(), t.len());
}
