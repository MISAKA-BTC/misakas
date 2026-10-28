//! Actual pre-object sweeps over stored auxiliary rows. Initial fixture/state
//! construction is deliberately outside each target phase's independent quota.
#![allow(warnings)]
use super::*;
use crate::palw_native_resource_v1::{NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1, NativeTransitionReservationV1};
use crate::palw_panel_var_v1::{PalwPanelLiabilityRecordV1, PalwSlashableLockV1};
use crate::palw_verification_v2::PalwSegmentMaskV2;
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
enum Phase {
    Prompt,
    Panel,
    Reward,
    Commitment,
}

fn liability(claim: Hash64, expiry: u64) -> PalwPanelLiabilityRecordV1 {
    PalwPanelLiabilityRecordV1 {
        claim_id: claim,
        work_id: claim,
        class_id: h64(1),
        execution_root: h64(2),
        output_root: h64(3),
        executor_bond: bond_key(1),
        voided_daa: None,
        void_reason: None,
        valid_signers: Vec::new(),
        locked_sompi: 0,
        expiry_daa: expiry,
        settled_at_final: 0,
        job_identity: h64(4),
        free_prompt: false,
        trace_root: h64(5),
        segment_count: 0,
        licence_door: None,
        basis_k: 0,
        g_res_sompi: 0,
        escrowed_reward: 0,
    }
}
fn fixture(phase: Phase) -> (PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1) {
    let p = params().with_rcore_plus_mirrors(Some(0), 12_900, Vec::new());
    let mut s = PalwChainStateV2::genesis();
    s.last_point = Some(ctx(1, 100, 1));
    let mut e = PalwTransitionExtrasV1::default();
    match phase {
        Phase::Prompt => {
            for (n, expiry) in [(1, 1999), (2, 2000), (3, 2001)] {
                s.fp_claim_prompts.insert(
                    (h64(n), bond_key(n), h64(99)),
                    PalwFpClaimPromptV1 {
                        prompt_tokens: 3,
                        prefix_root: h64(n),
                        prompt_token_ids: vec![1, 2, 3],
                        expires_daa: expiry,
                        consumed_state_root: ZERO_HASH64,
                        consumed_prefix_tokens: 0,
                    },
                );
            }
        }
        Phase::Panel => {
            e.audit_2026_09_23_active = true;
            for (n, expiry) in [(1, 10), (2, 2100), (3, 10)] {
                s.slashable_locks.insert(
                    (bond_key(n), h64(n)),
                    PalwSlashableLockV1 {
                        claim: h64(n),
                        amount: 5,
                        expiry_daa: expiry,
                        settled_at_final: 0,
                        attested: PalwSegmentMaskV2::NONE,
                        segments: 0,
                    },
                );
                if n != 3 {
                    s.panel_liabilities.insert(h64(n), liability(h64(n), expiry));
                }
            }
        }
        Phase::Reward => {
            for (n, reveal_until) in [(1, 1999), (2, 1999), (3, 2000)] {
                s.reward_pending.insert(
                    h64(n),
                    PalwPendingRewardV1 {
                        amount: 5,
                        reveal_until,
                        evidence_id: ZERO_HASH64,
                        best: (n == 1).then_some(PalwRewardWinnerV1 {
                            committed_daa: 20,
                            commitment: h64(20),
                            reporter: bond_key(1),
                            payload: h64(10),
                        }),
                    },
                );
            }
        }
        Phase::Commitment => {
            let mut b = TransitionBuilder::new(&s, &p, false, false, false, false, &e);
            for (n, daa) in [(1, 10), (2, 700), (3, 1500)] {
                b.write_reporter_commit(h64(n), Some(PalwReporterCommitV1 { reporter: bond_key(n), committed_daa: daa })).unwrap();
            }
            s = b.state;
        }
    }
    (s, p, e)
}
fn checker(bytes: u64) -> Arc<NativeDemandCheckerV1> {
    NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: bytes, database_bytes: 0 })
}
fn owned<'a>(
    s: &PalwChainStateV2,
    p: &'a PalwStateParamsV2,
    e: &'a PalwTransitionExtrasV1,
    c: &Arc<NativeDemandCheckerV1>,
) -> TransitionBuilder<'a> {
    let mut b = TransitionBuilder::new(s, p, false, false, false, false, e);
    b.native_reservation = NativeTransitionReservationV1::for_checker_v1(c);
    b
}
fn quote(phase: Phase, b: &mut TransitionBuilder<'_>) {
    match phase {
        Phase::Prompt => b.reserve_aux_sweep_vec_v2::<(Hash64, PalwBondKeyV2, Hash64)>(2, "expired FP prompt keys"),
        Phase::Panel => b.reserve_panel_sweep_workspace_v2(),
        Phase::Reward => b.reserve_aux_sweep_vec_v2::<(Hash64, PalwPendingRewardV1)>(2, "due reporter reward copies"),
        Phase::Commitment => b.reserve_aux_sweep_vec_v2::<Hash64>(2, "expired reporter commitment keys"),
    }
    .unwrap();
}
fn sweep(
    phase: Phase,
    b: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    point: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    match phase {
        Phase::Prompt => sweep_fp_prompt_rows(b, parent, point),
        Phase::Panel => sweep_panel_obligations(b, parent, point),
        Phase::Reward => sweep_reward_reveals(b, point),
        Phase::Commitment => sweep_reporter_commitments(b, point),
    }
}
fn check(phase: Phase) {
    let (s, p, e) = fixture(phase);
    let point = ctx(2, 2000, 2);
    let c = checker(u64::MAX);
    let mut b = owned(&s, &p, &e, &c);
    quote(phase, &mut b);
    let q = c.outstanding().0;
    assert!(q > 0);
    assert!(b.entries.is_empty());
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
    for limit in [0, q - 1] {
        let c = checker(limit);
        let mut b = owned(&s, &p, &e, &c);
        assert!(matches!(sweep(phase, &mut b, &s, &point), Err(PalwStateV2Error::NativeResource(_))), "{phase:?}");
        assert_eq!(b.state, s);
        assert_eq!(b.state.state_root(), s.state_root());
        assert!(b.entries.is_empty());
        assert_eq!(c.outstanding(), (0, 0));
    }
    let c = checker(q);
    let mut b = owned(&s, &p, &e, &c);
    quote(phase, &mut b); // Q admits caller scratch, not the later writers.
    assert_eq!(c.outstanding(), (q, 0));
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
    let mut raw = TransitionBuilder::new(&s, &p, false, false, false, false, &e);
    sweep(phase, &mut raw, &s, &point).unwrap();
    let c = checker(u64::MAX);
    let mut b = owned(&s, &p, &e, &c);
    sweep(phase, &mut b, &s, &point).unwrap();
    assert_eq!(b.state, raw.state);
    assert_eq!(b.state.state_root(), raw.state.state_root());
    assert_eq!(borsh::to_vec(&b.entries).unwrap(), borsh::to_vec(&raw.entries).unwrap());
    match phase {
        Phase::Prompt => assert_eq!(b.state.fp_claim_prompts.len(), 1),
        Phase::Panel => {
            assert_eq!(b.state.slashable_locks.len(), 1);
            assert_eq!(b.state.panel_liabilities.len(), 1);
        }
        Phase::Reward => {
            assert_eq!(b.state.reward_pending.len(), 1);
            assert_eq!(b.state.reporter_rewards[&h64(1)].amount, 5);
            assert_eq!(b.state.reporter_counters.awarded_sompi, 5);
            assert_eq!(b.state.reporter_counters.forgone_sompi, 5);
        }
        Phase::Commitment => {
            assert_eq!(b.state.reporter_commitments.len(), 1);
            assert_eq!(b.state.commitments_by_daa.len(), 1);
        }
    }
    let delta = PalwStateDeltaV2 { point, entries: b.entries.clone() };
    assert_eq!(apply_delta_v2(&s, &delta, &p).unwrap().state_root(), b.state.state_root());
    assert_eq!(revert_delta_v2(&b.state, &delta, &p).unwrap().state_root(), s.state_root());
    assert!(c.outstanding().0 >= q);
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
}

#[test]
fn aux_prompt_sweep_workspace_refusal_retry_and_journal_parity() {
    check(Phase::Prompt);
}
#[test]
fn aux_panel_sweep_workspace_refusal_retry_and_grouped_retirement() {
    check(Phase::Panel);
}
#[test]
fn aux_reward_sweep_workspace_refusal_retry_and_real_award() {
    check(Phase::Reward);
}
#[test]
fn aux_commitment_sweep_workspace_refusal_retry_and_index_undo() {
    check(Phase::Commitment);
}

#[test]
fn aux_sweep_workspace_dormant_and_same_epoch_skip_before_reservation() {
    for phase in [Phase::Prompt, Phase::Panel, Phase::Reward, Phase::Commitment] {
        let (s, _, mut e) = fixture(phase);
        let p = params(); // R-core+ dormant; epoch did not cross for prompt/panel.
        e.audit_2026_09_23_active = false;
        let c = checker(0);
        let mut b = owned(&s, &p, &e, &c);
        sweep(phase, &mut b, &s, &ctx(2, 101, 2)).unwrap();
        assert_eq!(b.state, s);
        assert!(b.entries.is_empty());
        assert_eq!(c.outstanding(), (0, 0));
    }
}
