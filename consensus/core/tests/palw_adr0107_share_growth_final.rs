//! **ADR-0107 at the V2 state fold: a class's cadence share grows only on work that reached
//! `Final`.**
//!
//! ADR-0054's growth rule reads the closed epoch's `produced_blocks`, which the transition
//! increments when an attempt is ACCEPTED and which no void gives back. As the budget's
//! anti-re-roll counter that is right; as the growth SIGNAL it grows a class on blocks nobody ever
//! verified. These tests drive the REAL transition — the function block validation folds — with
//! the RC's growth step (250‰) and floor reserve (20‰), and fold every scenario twice: with
//! `Params::palw_share_growth_final` dormant (the rule every shipped network runs today, pinned
//! here as the limitation it is) and armed.
//!
//! Run: `MISAKA_PALW_POW_FIXTURE=1 cargo test -p kaspa-consensus-core --test palw_adr0107_share_growth_final`

use kaspa_consensus_core::palw_attempt_v2::{PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2};
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj, PalwPanelSeatV2,
    PalwPwuRuleV2, PalwStateParamsV2, PalwTransitionExtrasV1, PalwVoidReasonV2, apply_palw_transition_v2_with_extras,
    palw_operator_id_v2,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;

const EPOCH_LENGTH: u64 = 1_000;
const PWU_PER_INFERENCE: u64 = 7_900;

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn base() -> Hash64 {
    h(0xBA5E)
}

fn entrant() -> Hash64 {
    h(0xE17)
}

fn bond_key() -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0xB0), index: 0 })
}

/// Short lattice windows — bind 10, receipt 10, challenge 20 — so one epoch holds a whole claim
/// life; the RC's growth, reserve and retirement otherwise.
fn params() -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 10, 10, 20, 500, EPOCH_LENGTH, base(), 4, 1_000, 100, 1_000, 0)
        .expect("state params")
        .with_claim_retirement_daa(3_000)
        .expect("retirement")
        .with_min_base_class_share_permille(20)
        .expect("floor reserve")
        .with_class_share_growth_v1(250)
        .expect("growth step")
}

fn registration(class: Hash64, share: u16) -> Obj {
    Obj::ClassRegistered {
        class_id: class,
        artifact_root: h(0xA7),
        slash_value_per_pwu: 1,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: PWU_PER_INFERENCE },
        initial_target: u128::MAX / 2,
        share_permille: share,
        activation_daa: 0,
        admission: None,
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
            // Any bytes: nothing before a panel reads these — which is the point of the ADR.
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

/// One chain, folded block by block with the fence as `armed` says.
struct Chain {
    state: PalwChainStateV2,
    params: PalwStateParamsV2,
    daa: u64,
    armed: bool,
}

impl Chain {
    fn new(armed: bool) -> Self {
        let mut chain = Chain { state: PalwChainStateV2::genesis(), params: params(), daa: 0, armed };
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
            registration(base(), 1_000),
            registration(entrant(), 1),
        ];
        chain.step(None, &genesis);
        chain
    }

    /// One block at the next DAA: an attempt of `class` if given, and `objects`.
    fn step(&mut self, class: Option<Hash64>, objects: &[Obj]) -> Option<Hash64> {
        self.daa += 1;
        let ctx =
            PalwBlockContextV2 { block: h(self.daa | 0x1000_0000), daa_score: self.daa, blue_score: self.daa, subsidy: 1_000_000 };
        let envelope = class.map(|c| {
            let target = self.state.class_target(&c).expect("a registered class has a target").target;
            attempt(c, kaspa_consensus_core::palw_pwu::palw_pwu_v1(target, PWU_PER_INFERENCE), self.daa)
        });
        let id = envelope.as_ref().map(|e| attempt_id_v2(&e.attempt));
        let extras = PalwTransitionExtrasV1 { share_growth_final_active: self.armed, ..Default::default() };
        let (next, _) = apply_palw_transition_v2_with_extras(
            &self.state,
            &self.params,
            &ctx,
            objects,
            envelope.as_ref(),
            false,
            false,
            false,
            false,
            &extras,
        )
        .unwrap_or_else(|e| panic!("block at daa {} refused: {e}", self.daa));
        next.assert_internal_consistency_v2(&self.params, false)
            .unwrap_or_else(|e| panic!("internal consistency after daa {}: {e}", self.daa));
        next.assert_deadline_consistency(&self.params).expect("deadline consistency");
        self.state = next;
        id
    }

    /// Floor blocks until the chain stands at `daa`.
    fn floor_until(&mut self, daa: u64) {
        while self.daa < daa {
            self.step(Some(base()), &[]);
        }
    }

    /// Bind and license `claim` at the next two blocks, the way a panel of one would.
    fn bind_and_license(&mut self, claim: Hash64) {
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(), operator_id: h(90) }];
        self.step(None, &[Obj::PanelBound { claim, anchor: h(77), seats }]);
        let receipts = vec![PalwSeatReceiptV2 {
            claim,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: bond_key(),
            signed_daa: 0,
            signature: Vec::new(),
        }];
        self.step(None, &[Obj::ReceiptLicensed { claim, receipts }]);
    }

    fn share(&self, class: &Hash64) -> u16 {
        self.state.class_share_permille(class).unwrap_or(0)
    }

    fn phase(&self, claim: &Hash64) -> PalwClaimPhaseV2 {
        self.state.claim(claim).expect("still recorded: retirement is 3000").phase.clone()
    }
}

/// An entrant block accepted early in epoch 0, never bound, voided at `BindTimeout`, and the
/// chain then crosses into epoch 1. Returns the chain after the crossing.
fn a_voided_block_then_the_boundary(armed: bool) -> Chain {
    let mut chain = Chain::new(armed);
    let budget = chain.state.epoch_budgets().and_then(|b| b.budget_blocks.get(&entrant()).copied());
    assert_eq!(budget, Some(1), "an entrant at the grant floor is budgeted one block");
    let claim = chain.step(Some(entrant()), &[]).expect("an attempt");
    chain.floor_until(EPOCH_LENGTH - 1);
    assert!(
        matches!(chain.phase(&claim), PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BindTimeout, .. }),
        "nobody bound it: {:?}",
        chain.phase(&claim)
    );
    assert_eq!(
        chain.state.epoch_counter(&entrant()).map(|c| (c.epoch_index, c.produced_blocks)),
        Some((0, 1)),
        "the void kept the block on the epoch's count — the budget's anti-re-roll rule, unchanged"
    );
    chain.step(Some(base()), &[]); // DAA 1000: the crossing
    chain
}

/// **The limitation, pinned while it is the network's rule.** Dormant, the voided, never-verified
/// block grows the entrant a step — exactly the review's example. This must stay true until the
/// fence is armed on a network; it is what every shipped preset folds today.
#[test]
fn dormant_a_voided_block_still_grows_the_class_it_filled() {
    let chain = a_voided_block_then_the_boundary(false);
    assert_eq!(chain.share(&entrant()), 2, "1‰ → 2‰ on a block that never reached a panel");
}

/// **Armed, the same block earns nothing.** And the entrant does NOT decay: it produced, so it
/// is not idle — the fence changes who grows and nothing else.
#[test]
fn armed_a_voided_block_earns_no_growth_and_costs_no_decay() {
    let dormant = a_voided_block_then_the_boundary(false);
    let armed = a_voided_block_then_the_boundary(true);
    assert_eq!(armed.share(&entrant()), 1, "no Final work, no growth");
    assert_eq!(armed.share(&base()) + armed.share(&entrant()), 1_000, "the table still sums to its denominator");
    assert_eq!(dormant.share(&base()), armed.share(&base()) - 1, "the one permille stayed with the floor");
}

/// **Armed, Final work earns exactly what it earned before.** The entrant's one block is bound,
/// licensed and matured inside epoch 0; the crossing then grows it a step, as the dormant rule
/// does — verified work is not taxed by the fence.
#[test]
fn armed_final_work_earns_the_same_growth_as_before() {
    for armed in [false, true] {
        let mut chain = Chain::new(armed);
        let claim = chain.step(Some(entrant()), &[]).expect("an attempt");
        chain.bind_and_license(claim);
        chain.floor_until(100); // well past the 20-DAA challenge window: the sweep matures it
        assert!(matches!(chain.phase(&claim), PalwClaimPhaseV2::Final { .. }), "armed={armed}: {:?}", chain.phase(&claim));
        chain.floor_until(EPOCH_LENGTH - 1);
        chain.step(Some(base()), &[]);
        assert_eq!(chain.share(&entrant()), 2, "armed={armed}: a verified block that filled the budget grows the class");
    }
}

/// **A claim still `Provisional` at the boundary is not yet evidence.** Accepted late in epoch 0
/// it cannot finalize before the crossing: dormant it grows the class anyway (the review's second
/// case), armed it does not — and when the same claim finalizes in epoch 1 it counts toward epoch
/// 1's boundary, so verified work is late rather than lost.
#[test]
fn armed_growth_waits_for_the_epoch_in_which_the_work_finalizes() {
    for armed in [false, true] {
        let mut chain = Chain::new(armed);
        chain.floor_until(990);
        let late = chain.step(Some(entrant()), &[]).expect("an attempt"); // DAA 991
        chain.bind_and_license(late); // bound 992, licensed 993: Final only past 1013
        chain.floor_until(EPOCH_LENGTH - 1);
        assert!(matches!(chain.phase(&late), PalwClaimPhaseV2::ReceiptLicensed { .. }));
        chain.step(Some(base()), &[]); // DAA 1000: epoch 0 closes
        assert_eq!(chain.share(&entrant()), if armed { 1 } else { 2 }, "armed={armed}: at the first crossing");

        let after_first = chain.share(&entrant());

        // Epoch 1: the late claim finalizes early in it, and the entrant fills its epoch-1 budget
        // — whatever the first crossing made it — with blocks that are bound, licensed and matured
        // inside the epoch.
        chain.floor_until(1_050);
        assert!(matches!(chain.phase(&late), PalwClaimPhaseV2::Final { .. }), "armed={armed}: {:?}", chain.phase(&late));
        let quota = chain.state.epoch_budgets().and_then(|b| b.budget_blocks.get(&entrant()).copied()).unwrap_or(0);
        assert!(quota >= 1, "armed={armed}: the entrant is budgeted in epoch 1");
        for _ in 0..quota {
            let claim = chain.step(Some(entrant()), &[]).expect("an attempt");
            chain.bind_and_license(claim);
        }
        chain.floor_until(2 * EPOCH_LENGTH - 1);
        chain.step(Some(base()), &[]); // DAA 2000: epoch 1 closes
        let grown = chain.share(&entrant());
        assert!(grown > after_first, "armed={armed}: epoch 1's verified work grows the class ({after_first}‰ → {grown}‰)");
    }
}
