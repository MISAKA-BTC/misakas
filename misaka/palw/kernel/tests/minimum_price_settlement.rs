//! Actual typed kernel execution and the settlement consumer. These tests are
//! source only until the combined validation phase; no native coinbase claim is
//! made by this in-process bond-book fixture.
#![allow(warnings)]
mod common;
use common::{
    chain::T,
    ledger_world::{PRODUCER, World},
};
use misaka_palw_kernel::{ledger::LedgerEventV1 as E, settle::SettlementKindV1 as K};

#[test]
fn funded_minimum_pays_once_and_a_later_branch_with_insufficient_cap_releases_neutrally() {
    let mut world = World::new();
    let job = world.post_job(2, &[3, 17, 9], 3, 0x62);
    let honest = world.honest(&job, 3);
    let claim = honest.claim.id();
    world.block(10, vec![honest.tx, T::PanelCovered { claim }]);
    world.l.consumer_minimum_payouts_v1.insert(claim, 7);
    world.l.consumer_final_reward_caps.insert(claim, 7);
    let before = world.l.clone();
    let consumer = world.consumer.clone();
    let block_count = world.blocks.len();
    let event_count = world.events.len();
    let final_events = world.block(60, vec![]);
    assert!(final_events.contains(&E::Final { claim, reward: 7 }));
    assert_eq!(world.consumer.paid(&PRODUCER), 7);
    assert!(!world.l.job_escrows.contains_key(&job.id()));
    world.block(61, vec![]);
    assert_eq!(world.consumer.paid(&PRODUCER), 7, "an already-paid Final never pays twice");
    // Restore the actual ledger+consumer snapshot on the other branch. Its
    // monetary cap differs, while the accepted minimum remains seven.
    world.l = before;
    world.consumer = consumer;
    world.blocks.truncate(block_count);
    world.events.truncate(event_count);
    world.l.consumer_final_reward_caps.insert(claim, 6);
    let neutral = world.block(60, vec![]);
    assert!(neutral.contains(&E::Final { claim, reward: 0 }));
    assert_eq!(world.consumer.paid(&PRODUCER), 0);
    assert!(world.consumer.settlements.iter().any(|(_, s)| s.claim == Some(claim) && s.kind == K::ReleaseJobEscrow && s.amount == 7));
    assert!(!world.consumer.settlements.iter().any(|(_, s)| s.claim == Some(claim) && s.kind == K::FinalReward && s.amount > 0));
    assert!(!world.l.job_escrows.contains_key(&job.id()));
    world.block(61, vec![]);
    assert_eq!(world.consumer.paid(&PRODUCER), 0, "neutral escrow release cannot later become a clipped payment");
}
