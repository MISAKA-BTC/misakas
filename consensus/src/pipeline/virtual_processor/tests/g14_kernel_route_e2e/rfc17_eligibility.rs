//! **RFC-0017 on the real node** (2026-10-15, test source; NOT RUN in the change that wrote it — no cargo invocation was allowed;
//! nothing here is a PASS record).
//!
//! * **A-2 at the door:** below `palw_model_eligibility_v1` an exam commit (inner 74) is refused by the processor's inner-kind gate
//!   — the mempool takes no carrier for it — and the chain runs on with no aux 131 row.
//! * **Genesis classes past the fence** (user decision U-6, 2026-10-15: no permanent genesis exemption except the non-reward BASE-0
//!   liveness floor): an armed node and an unarmed twin fed the same script of heartbeats and attempts admit the same claims and
//!   commit the same PALW state roots block by block — inside the one-renewal-period grace a rewarded genesis class keeps its rights
//!   (U-5) — and the registry read reports the BASE-0 floor `not-governed` and every other genesis class `eligible` (its pre-fence
//!   grace, renewal due one period after the fence), both with the three rights.
//! * **An exam of the BASE-0 floor is refused** through the node: no exam, no attempt, no fee.
//!
//! What this file does NOT carry, said here so it is not mistaken for coverage: a non-genesis class registered past the fence through
//! the node (the registration carriage and seat readiness of `g14_registration_e2e`) and its probation → exam → promotion → lapse walk.
//! That walk is covered through the actual fold in `consensus/core/src/palw_state_v2/tests/model_eligibility_fold_v1.rs`; its node
//! E2E is RFC-0017 §10's open item.
//!
//! Like the rest of this suite the fence is armed WITHOUT its validation (which refuses it on every height).
use super::*;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_model_eligibility_v1::PALW_MODEL_ELIGIBILITY_TABLE_V1;
use misaka_palw_kernel::model_eligibility_v1::{
    ELIGIBILITY_EXAM_COMMIT_VERSION_V1, EligibilityExamCommitV1, EligibilityExamPurposeV1,
};

/// The fence of the armed worlds: past the kernel route's first blocks, before the scripted attempts.
const FENCE: u64 = 6;

/// The kernel network ([`kernel_config`]) with `palw_model_eligibility_v1` armed at `fence` (re-mirrored onto the bundle the chain
/// is built from), or unarmed.
fn rfc17_config(fence: Option<u64>) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, bundle, premine, floats) = kernel_config();
    let Some(at) = fence else { return (config, bundle, premine, floats) };
    let mut params = config.params.clone();
    params.palw_model_eligibility_v1 = Some(ForkActivation::new(at));
    params.sync_palw_model_eligibility_v1();
    assert!(
        params.validate_palw_model_eligibility_v1().is_err(),
        "the real validation still refuses the fence; only this harness bypasses it"
    );
    let PalwConsensusMode::ConsensusV2(armed) = &params.palw_consensus_mode else { unreachable!("ConsensusV2") };
    assert!(armed.state.model_eligibility_v1().is_some(), "the mirror is set");
    let bundle = armed.clone();
    (Config::new(params), bundle, premine, floats)
}

fn base_class(net: &Net) -> Hash64 {
    net.bundle.state.base_class_id()
}

fn exam_commit(net: &Net, card: usize, class: Hash64) -> K {
    K::CommitEligibilityExamV1 {
        commit: EligibilityExamCommitV1 {
            version: ELIGIBILITY_EXAM_COMMIT_VERSION_V1,
            network: net.domain.as_bytes(),
            committer: net.kid(card),
            class_id: class.as_bytes(),
            purpose: EligibilityExamPurposeV1::Promotion,
            claims_root: [0xC1; 64],
            claims_count: 16,
            attempt_index: 0,
        },
    }
}

fn eligibility_rows(net: &Net) -> usize {
    net.api().map_or(0, |route| {
        route.aux.range((PALW_MODEL_ELIGIBILITY_TABLE_V1, Vec::new())..(PALW_MODEL_ELIGIBILITY_TABLE_V1 + 1, Vec::new())).count()
    })
}

#[tokio::test]
async fn rfc17_an_exam_commit_below_the_fence_is_refused_at_the_door_and_the_chain_runs_on() {
    let mut net = Net::over_cfg(rfc17_config(None), TestConsensus::new);
    net.beat_to(3).await;
    let object = net.route(0, &exam_commit(&net, 0, base_class(&net)));
    let tx = net.carrier(0, &object);
    assert!(net.mempool(&tx).is_err(), "inner 74 below palw_model_eligibility_v1 is the live build's undecodable kind: refused");
    net.beat_to(net.daa() + 3).await;
    assert_eq!(eligibility_rows(&net), 0, "no aux 131 row below the fence");
}

#[tokio::test]
async fn rfc17_genesis_classes_stay_ungoverned_past_the_fence_byte_for_byte() {
    let mut armed = Net::over_cfg(rfc17_config(Some(FENCE)), TestConsensus::new);
    let mut twin = Net::over_cfg(rfc17_config(None), TestConsensus::new);
    for net in [&mut armed, &mut twin] {
        net.beat_to(FENCE + 2).await;
    }
    for card in [0usize, 1, 0] {
        let (armed_ttpb, twin_ttpb) = (armed.ttpb(), twin.ttpb());
        let (_, a) = armed.chain.attempt(card, armed_ttpb, Vec::new(), &|_| true).await;
        let (_, b) = twin.chain.attempt(card, twin_ttpb, Vec::new(), &|_| true).await;
        let (sa, sb) = (armed.chain.tip_state().1, twin.chain.tip_state().1);
        assert_eq!(sa.claim(&a).is_some(), sb.claim(&b).is_some(), "card {card}: the same admission on both nodes");
        assert_eq!(sa.state_root(), sb.state_root(), "card {card}: the same PALW state root (genesis classes are ungoverned)");
    }
    let read = armed.chain.ctx.consensus.palw_model_registry_v1();
    let base = base_class(&armed);
    if let Some(read) = read {
        for class in &read.classes {
            if class.class_id == base {
                assert_eq!(class.eligibility.stage, "not-governed", "the BASE-0 floor is the one dedicated exception (U-6)");
            } else {
                assert_eq!(
                    class.eligibility.stage, "eligible",
                    "a rewarded genesis class is a pre-fence class in its grace (U-5/U-6)"
                );
                assert!(class.eligibility.next_renewal_daa > FENCE, "its renewal is due one period after the fence");
            }
            assert!(class.eligibility.subsidy && class.eligibility.block_right && class.eligibility.final_weight);
        }
    }
    assert_eq!(eligibility_rows(&armed), 0, "nothing wrote an eligibility row");
}

#[tokio::test]
async fn rfc17_an_exam_of_a_genesis_class_is_refused_through_the_node() {
    let mut net = Net::over_cfg(rfc17_config(Some(FENCE)), TestConsensus::new);
    net.beat_to(FENCE + 2).await;
    let before = net.collateral(0);
    let object = net.route(0, &exam_commit(&net, 0, base_class(&net)));
    let tx = net.carrier(0, &object);
    // The door may refuse it (the rehearsal folds the object alone); if it does not, the block's fold drops it.
    if net.mempool(&tx).is_ok() {
        let ttpb = net.ttpb();
        net.chain.heartbeat(ttpb, vec![tx]).await;
        net.chain.heartbeat(ttpb, Vec::new()).await;
    }
    assert_eq!(eligibility_rows(&net), 0, "no exam, no lineage, no network row");
    assert_eq!(net.collateral(0), before, "no attempt fee is burned for a refused commit");
}
