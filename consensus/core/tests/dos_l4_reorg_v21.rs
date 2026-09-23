//! **LANE L4 (area 8) — the v21 fields through the REAL fold, a full revert, and restart.**
//!
//! Drives `apply_palw_transition_v2_with_extras` (the function block validation folds) with the
//! 2026-09-23 fence armed, `settled_anchor_depth = 30` and the objective-offence rules on, through
//! apply -> bind -> license (past the fence the second clock ticks HERE and the licence's DAA joins
//! the anchor ring — 2026-09-24 DoS audit, fix #2) -> Final (lock + liability re-stamped with
//! `settled_at_final`; the Final itself ticks nothing) -> retire attempts -> DAA far past every
//! window. Every block's delta is kept;
//! at the end the whole chain is reverted newest-first and each parent root must come back exactly,
//! and every intermediate state is round-tripped through the borsh carriage (the restart / pruning
//! import path, schema v21 tail 0xB3).
//!
//! It also measures, on the real fold, that (a) every voided floor attempt leaves a permanent
//! `panel_liabilities` row, and (b) a seat whose lock was written before a licence-less stretch
//! cannot REQUEST retirement until the liveness escape (fix #3): `2 × window_court` after the last
//! licence, refused at `E − 1`, accepted at `E`. (The audit measured "at any DAA" — the unbounded
//! freeze `6bb8c844` closed.)
//!
//! Harness copied from `palw_adr0107_share_growth_final.rs` (panel of one, short windows).

use kaspa_consensus_core::palw_attempt_v2::{PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2};
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwBondStatusV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj,
    PalwPanelSeatV2, PalwPwuRuleV2, PalwStateCarriageV2, PalwStateDeltaV2, PalwStateParamsV2, PalwStateV2Error,
    PalwTransitionExtrasV1, apply_palw_transition_v2_with_extras, palw_operator_id_v2, revert_delta_v2,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;

const EPOCH_LENGTH: u64 = 1_000;
const PWU_PER_INFERENCE: u64 = 7_900;
const DEPTH: u64 = 30;

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn base() -> Hash64 {
    h(0xBA5E)
}

fn bond_key() -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0xB0), index: 0 })
}

fn params() -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 10, 10, 20, 500, EPOCH_LENGTH, base(), 4, 1_000, 100, 1_000, 0)
        .expect("state params")
        .with_claim_retirement_daa(3_000)
        .expect("retirement")
}

fn extras() -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 {
        audit_2026_09_23_active: true,
        settled_anchor_depth: Some(DEPTH),
        objective_offence_daa: Some(0),
        ..Default::default()
    }
}

fn attempt(class: Hash64, pwu: u64, nonce: u64) -> PalwAttemptEnvelopeV2 {
    PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: h(0xD0),
            challenge: h(nonce ^ 0x00C0_FFEE),
            class_id: class,
            executor_bond: bond_key().0,
            executor_pubkey: vec![7u8; 32],
            operator_id: palw_operator_id_v2(&[21u8; 8]),
            artifact_root: h(0xA7),
            trace_root: h(nonce ^ 0x7A),
            output_root: h(nonce ^ 0x00FF),
            pwu,
            trace_manifest_root: h(0xD0),
            trace_chunk_count: 8,
            trace_retention_daa: u64::MAX,
            execution_root: h(nonce ^ 0x4E),
        },
        signature: vec![9u8; 64],
    }
}

struct Chain {
    state: PalwChainStateV2,
    params: PalwStateParamsV2,
    daa: u64,
    /// (parent root, delta) per block, oldest first.
    log: Vec<(Hash64, PalwStateDeltaV2)>,
    carriage_checks: u64,
}

impl Chain {
    fn new() -> Self {
        let mut chain = Chain { state: PalwChainStateV2::genesis(), params: params(), daa: 0, log: Vec::new(), carriage_checks: 0 };
        let genesis = vec![
            Obj::BondRegistered {
                bond: bond_key(),
                pubkey: vec![7u8; 32],
                operator_pubkey: vec![21u8; 8],
                collateral: 1_000_000_000_000_000,
                payout_payload: h(0x9A4),
                capable_classes: Default::default(),
                signature: vec![9u8; 64],
            },
            Obj::ClassRegistered {
                class_id: base(),
                artifact_root: h(0xA7),
                slash_value_per_pwu: 1,
                pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: PWU_PER_INFERENCE },
                initial_target: u128::MAX / 2,
                share_permille: 1_000,
                activation_daa: 0,
                admission: None,
            },
        ];
        chain.step(false, &genesis).expect("genesis");
        chain
    }

    fn try_step(&mut self, with_attempt: bool, objects: &[Obj]) -> Result<Option<Hash64>, PalwStateV2Error> {
        let daa = self.daa + 1;
        let ctx = PalwBlockContextV2 { block: h(daa | 0x1000_0000), daa_score: daa, blue_score: daa, subsidy: 1_000_000 };
        let envelope = with_attempt.then(|| {
            let target = self.state.class_target(&base()).expect("target").target;
            attempt(base(), kaspa_consensus_core::palw_pwu::palw_pwu_v1(target, PWU_PER_INFERENCE), daa)
        });
        let id = envelope.as_ref().map(|e| attempt_id_v2(&e.attempt));
        let (next, delta) = apply_palw_transition_v2_with_extras(
            &self.state,
            &self.params,
            &ctx,
            objects,
            envelope.as_ref(),
            false,
            false,
            false,
            false,
            &extras(),
        )?;
        // Restart / pruning import: the carriage must round-trip to the same root.
        if daa % 7 == 0 || !objects.is_empty() {
            let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&next)).expect("encode");
            let back: PalwStateCarriageV2 = borsh::from_slice(&bytes).expect("decode");
            let restored = back.into_state(&self.params, Some(next.state_root())).expect("carriage restores the same root");
            assert_eq!(restored.settled_attempt_finals(), next.settled_attempt_finals());
            assert_eq!(restored.recent_anchor_daas(), next.recent_anchor_daas(), "the anchor ring rides the 0xB3 tail");
            self.carriage_checks += 1;
        }
        self.log.push((self.state.state_root(), delta));
        self.state = next;
        self.daa = daa;
        Ok(id)
    }

    fn step(&mut self, with_attempt: bool, objects: &[Obj]) -> Result<Option<Hash64>, PalwStateV2Error> {
        self.try_step(with_attempt, objects)
    }

    fn floor_until(&mut self, daa: u64) {
        while self.daa < daa {
            self.step(true, &[]).expect("floor block");
        }
    }

    fn empty_until(&mut self, daa: u64) {
        while self.daa < daa {
            self.step(false, &[]).expect("empty block");
        }
    }

    fn bind_and_license(&mut self, claim: Hash64) {
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(), operator_id: h(90) }];
        self.step(false, &[Obj::PanelBound { claim, anchor: h(77), seats }]).expect("bind");
        let receipts = vec![PalwSeatReceiptV2 {
            claim,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: bond_key(),
            signed_daa: 0,
            signature: Vec::new(),
        }];
        self.step(false, &[Obj::ReceiptLicensed { claim, receipts }]).expect("license");
    }

    fn liabilities(&self) -> usize {
        PalwStateCarriageV2::from_state(&self.state).panel_liabilities.len()
    }
}

#[test]
fn v21_fields_revert_exactly_and_round_trip_through_the_carriage() {
    let mut chain = Chain::new();

    // Three claims to Final. Every other floor attempt is never bound and voids at BindTimeout.
    let mut finals = Vec::new();
    for _ in 0..3 {
        let claim = chain.step(true, &[]).expect("attempt").expect("id");
        chain.bind_and_license(claim);
        chain.floor_until(chain.daa + 25);
        assert!(matches!(chain.state.claim(&claim).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "{:?}", chain.state.claim(&claim));
        finals.push(claim);
    }
    assert_eq!(chain.state.settled_attempt_finals(), 3, "three licences, three ticks — the Finals tick nothing past the fence");
    assert_eq!(chain.state.recent_anchor_daas().len(), 3, "each licence's DAA is in the ring");
    let lock = chain.state.slashable_lock(bond_key(), finals[2]).copied().expect("the Valid seat is locked");
    println!("lock on the 3rd Final: expiry_daa {} settled_at_final {}", lock.expiry_daa, lock.settled_at_final);
    assert_eq!(lock.settled_at_final, 3, "re-stamped at the Final, after its own licence ticked");

    // Every voided floor attempt left a liability row too — until 2026-09-24 DoS audit #12 (a): a
    // `BindTimeout` that no `Valid` signed writes none past the audit fence. Only the Finals' rows.
    let voided_floor = chain.liabilities() - finals.len();
    println!("after DAA {}: {} liabilities ({} from never-bound floor voids)", chain.daa, chain.liabilities(), voided_floor);
    assert_eq!(voided_floor, 0, "a never-bound BindTimeout void persists no liability");

    // The freeze is bounded (fix #3): with no licence after the third, the lock is past its DAA
    // expiry but live on the second clock until E − 1 = last licence + 2 × window_court − 1, and
    // the DAA clock alone releases it at E.
    let last = *chain.state.recent_anchor_daas().last().expect("the last licence");
    let e = last + 2 * chain.params.window_court();
    assert!(lock.expiry_daa < e - 1, "only the second clock still holds the lock at E − 1");
    let retire = [Obj::BondRetireRequested { bond: bond_key(), signature: vec![9u8; 64] }];
    chain.empty_until(e - 2);
    let refused = chain.try_step(false, &retire);
    println!("retire request at DAA {} = E − 1 (lock DAA expiry {}): {refused:?}", chain.daa + 1, lock.expiry_daa);
    assert!(matches!(refused, Err(PalwStateV2Error::BondRetireWhileSlashableLocked { .. })), "E − 1: still bound: {refused:?}");
    assert!(matches!(chain.state.bond(&bond_key()).unwrap().status, PalwBondStatusV2::Active));
    chain.empty_until(e - 1);
    chain.step(false, &retire).expect("E: the liveness escape releases the lock on the DAA clock alone");
    assert!(matches!(chain.state.bond(&bond_key()).unwrap().status, PalwBondStatusV2::Retiring { .. }));
    // Then 10,000 DAA of blocks: the liability rows stay (finding 13, not this fix's).
    chain.empty_until(e + 10_000);
    let rows_before_retire = chain.liabilities();
    assert!(rows_before_retire >= voided_floor, "and nothing was pruned by 10,000 DAA of blocks");

    // Now the full revert: newest-first, every parent root exactly.
    let total = chain.log.len();
    let mut state = chain.state.clone();
    while let Some((parent_root, delta)) = chain.log.pop() {
        state = revert_delta_v2(&state, &delta, &chain.params).expect("revert");
        assert_eq!(state.state_root(), parent_root, "revert of block at DAA {} must restore its parent", delta.point.daa_score);
    }
    assert_eq!(state.state_root(), PalwChainStateV2::genesis().state_root(), "back to genesis");
    assert_eq!(state.settled_attempt_finals(), 0);
    println!("reverted {total} blocks exactly; {} carriage round-trips matched their roots", chain.carriage_checks);
}
