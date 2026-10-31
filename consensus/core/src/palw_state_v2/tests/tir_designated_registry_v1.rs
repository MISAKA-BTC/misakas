//! ADR-0191 step B: the designated-answer registry as a PALW state field. Tag 245 folds into it only when the fence's mirror
//! (`PalwTransitionExtrasV1::tir_designated_answers_active`) is set; below it the event is a strict no-op and the state root is
//! byte-identical to a block that carries no such event.
#![allow(warnings)]
use super::*;
use crate::palw_tir_designated_answers_v1 as d;

fn key() -> d::PalwTirRequestKeyV1 {
    d::PalwTirRequestKeyV1 { claim: [1; 32], root: [2; 32], coordinates: 7 }
}

fn request() -> d::PalwTirDesignatedRequestV1 {
    d::PalwTirDesignatedRequestV1 { key: key(), designated: vec![[11; 32], [12; 32]], obligation_daa: 10, opened_daa: 100 }
}

fn event(event: d::PalwTirDesignatedEventV1) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::TirDesignatedEventV1 { event }
}

/// One block (`blue` strictly increasing along the chain) carrying `objects`, with the fence's mirror set or not.
fn block(
    parent: &PalwChainStateV2,
    daa: u64,
    blue: u64,
    objects: &[PalwConsensusObjectV2],
    armed: bool,
) -> Result<PalwChainStateV2, PalwStateV2Error> {
    let extras = PalwTransitionExtrasV1 { tir_designated_answers_active: armed, ..Default::default() };
    apply_palw_transition_v2_with_extras(parent, &params(), &ctx(1, daa, blue), objects, None, false, false, false, false, &extras)
        .map(|(state, _)| state)
}

#[test]
fn tir_designated_event_is_ignored_while_the_fence_is_none() {
    let genesis = PalwChainStateV2::genesis();
    let empty = block(&genesis, 100, 1, &[], false).expect("an empty block folds");
    for object in [
        event(d::PalwTirDesignatedEventV1::Open(request())),
        event(d::PalwTirDesignatedEventV1::Answer { key: key(), custodian: [12; 32], body: [3; 32], at_daa: 105 }),
        event(d::PalwTirDesignatedEventV1::FalseValid { key: key(), signer: [11; 32] }),
    ] {
        let after = block(&genesis, 100, 1, &[object], false).expect("a dormant event is a no-op, not a refusal");
        assert!(after.tir_designated.records.is_empty(), "no record is written below the fence");
        assert_eq!(after.state_root(), empty.state_root(), "the root is byte-identical to a block without the event");
    }
}

#[test]
fn tir_designated_event_folds_into_the_registry_once_the_fence_is_armed() {
    let genesis = PalwChainStateV2::genesis();
    let opened = block(&genesis, 100, 1, &[event(d::PalwTirDesignatedEventV1::Open(request()))], true).expect("the request opens");
    assert_eq!(opened.tir_designated.records.get(&key()).map(|record| &record.request), Some(&request()));
    assert_ne!(opened.state_root(), genesis.state_root(), "a written record moves the root");

    let answer = d::PalwTirDesignatedEventV1::Answer { key: key(), custodian: [12; 32], body: [3; 32], at_daa: 105 };
    let answered = block(&opened, 105, 2, &[event(answer)], true).expect("a designated custodian's answer is accepted");
    let record = answered.tir_designated.records.get(&key()).expect("the record is in the registry");
    assert_eq!(
        d::palw_tir_designated_outcome_v1(record, 105),
        Ok(d::PalwTirDesignatedOutcomeV1::Answered { by: [12; 32], body: [3; 32] })
    );
}

#[test]
fn tir_designated_event_repeated_is_idempotent() {
    let genesis = PalwChainStateV2::genesis();
    let opened = block(&genesis, 100, 1, &[event(d::PalwTirDesignatedEventV1::Open(request()))], true).expect("the request opens");
    // A second Open of the same request is refused by the registry's rules, so the block is refused and nothing is written.
    assert!(block(&opened, 100, 2, &[event(d::PalwTirDesignatedEventV1::Open(request()))], true).is_err());

    // A repeated liability mark changes nothing after its first application.
    let mark = || event(d::PalwTirDesignatedEventV1::FalseValid { key: key(), signer: [11; 32] });
    let marked = block(&opened, 110, 2, &[mark()], true).expect("the first mark folds");
    let again = block(&marked, 110, 3, &[mark()], true).expect("the repeat is accepted and changes nothing");
    // The repeat writes nothing: its root is that of the same block carrying no event at all.
    let nothing = block(&marked, 110, 3, &[], true).expect("the empty block folds");
    assert_eq!(again.tir_designated, marked.tir_designated);
    assert_eq!(again.state_root(), nothing.state_root());
    assert!(again.tir_designated.records[&key()].liable.contains(&[11; 32]));
}
