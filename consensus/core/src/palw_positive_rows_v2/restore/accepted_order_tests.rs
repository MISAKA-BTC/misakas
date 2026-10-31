//! Accepted-order consistency only; different blocks still need native ancestry.
use super::super::tests::{all_rows, encoded_rows, origin};
use super::*;
use misaka_palw_positive_contract::messages::WitnessStatus;

fn event(n: u32) -> AcceptedOrigin {
    AcceptedOrigin { event_ordinal: n, ..origin() }
}

fn check(rows: &[NativeRowValue]) -> Result<PositiveRestoreReportV2, PositiveRowErrorV2> {
    let aux = encoded_rows(rows).into_iter().map(|(table, key, value)| ((table, key), value)).collect();
    validate_positive_aux_restore_v2(&aux)
}

/// Origins are deliberately outside the signed payloads. Keep genuine signed
/// rows and change only their accepted-history DTOs, never a signature stub.
fn ordered_rows() -> Vec<NativeRowValue> {
    let mut rows = all_rows();
    for row in &mut rows {
        match &mut row.body {
            NativeRowBody::Result(r) => r.origin = event(10),
            NativeRowBody::Witness(r) => r.origin = event(20),
            NativeRowBody::PacketReceipt(r) => r.origin = event(30),
            NativeRowBody::Coverage(r) => r.last_event = event(30),
            NativeRowBody::ServiceReceipt(r) => {
                r.duty.origin = event(10);
                r.response.origin = event(20);
                r.settlement.as_mut().unwrap().origin = event(20);
            }
            NativeRowBody::Funding(r) => r.funding_origin = event(10),
            NativeRowBody::Case(r) => r.last_event = event(30),
            NativeRowBody::JournalHead(r) => {
                r.sequence = 2;
                r.history.origin = event(30);
            }
            _ => (),
        }
    }
    rows
}

fn after_origin(rows: &mut [NativeRowValue], pair: usize) -> &mut AcceptedOrigin {
    rows.iter_mut()
        .find_map(|row| match (&mut row.body, pair) {
            (NativeRowBody::Witness(r), 0) => Some(&mut r.origin),
            (NativeRowBody::PacketReceipt(r), 1) => Some(&mut r.origin),
            (NativeRowBody::ServiceReceipt(r), 2) => Some(&mut r.response.origin),
            _ => None,
        })
        .unwrap()
}

#[test]
fn real_signed_restore_requires_strict_result_witness_packet_and_duty_response_order() {
    let baseline = ordered_rows();
    assert_eq!(check(&baseline).unwrap().rows, baseline.len() as u64);
    // Event ordinals restart in the next accepted transaction.
    let mut next_transactions = baseline.clone();
    let in_tx = |tx| AcceptedOrigin { accepted_tx_ordinal: tx, event_ordinal: 0, ..origin() };
    for row in &mut next_transactions {
        match &mut row.body {
            NativeRowBody::Witness(r) => r.origin = in_tx(5),
            NativeRowBody::PacketReceipt(r) => r.origin = in_tx(6),
            NativeRowBody::Coverage(r) => r.last_event = in_tx(6),
            NativeRowBody::ServiceReceipt(r) => {
                r.response.origin = in_tx(5);
                r.settlement.as_mut().unwrap().origin = in_tx(5);
            }
            NativeRowBody::Case(r) => r.last_event = in_tx(6),
            NativeRowBody::JournalHead(r) => r.history.origin = in_tx(6),
            _ => (),
        }
    }
    assert!(check(&next_transactions).is_ok());
    for (pair, before) in [event(10), event(20), event(10)].into_iter().enumerate() {
        for equal in [false, true] {
            let mut rows = baseline.clone();
            *after_origin(&mut rows, pair) = AcceptedOrigin { event_ordinal: before.event_ordinal - u32::from(!equal), ..before };
            assert!(check(&rows).is_err(), "strict pair {pair}, equal={equal}");
        }
        // Transaction order dominates the event ordinal within a transaction.
        let mut rows = baseline.clone();
        *after_origin(&mut rows, pair) =
            AcceptedOrigin { accepted_tx_ordinal: before.accepted_tx_ordinal - 1, event_ordinal: u32::MAX, ..before };
        assert!(check(&rows).is_err(), "reverse transaction order for pair {pair}");
    }
}

#[test]
fn same_accepting_block_rejects_daa_or_blue_score_disagreement_for_every_strict_pair() {
    let baseline = ordered_rows();
    for pair in 0..3 {
        for change_daa in [false, true] {
            let mut rows = baseline.clone();
            let after = after_origin(&mut rows, pair);
            if change_daa {
                after.accepted_daa += 1;
            } else {
                after.accepted_blue_score += 1;
            }
            assert!(check(&rows).is_err(), "pair {pair}, DAA={change_daa}");
        }
    }
}

#[test]
fn different_accepting_blocks_do_not_gain_a_fictitious_global_transaction_order() {
    let baseline = ordered_rows();
    for pair in 0..3 {
        let mut rows = baseline.clone();
        let after = after_origin(&mut rows, pair);
        after.accepting_block = NativeHash64([101; 64]);
        after.accepted_tx_ordinal = 0;
        after.event_ordinal = 0;
        after.accepted_daa = 1;
        after.accepted_blue_score = 1;
        assert!(check(&rows).is_ok(), "cross-block order remains native-history work, pair {pair}");
    }
}

#[test]
fn empty_coverage_can_share_the_witness_event_but_cannot_precede_it() {
    let mut baseline = ordered_rows();
    baseline.retain(|r| !matches!(r.body, NativeRowBody::PacketReceipt(_)));
    for row in &mut baseline {
        match &mut row.body {
            NativeRowBody::Case(r) => {
                r.status = ClaimStatus::Pending;
                r.witness_status = WitnessStatus::Checking;
            }
            NativeRowBody::Coverage(r) => {
                r.next_relation = 0;
                r.state = CoverageState::Checking;
                r.last_event = event(20);
            }
            _ => (),
        }
    }
    assert!(check(&baseline).is_ok());
    for variant in 0..3 {
        let mut rows = baseline.clone();
        for row in &mut rows {
            if let NativeRowBody::Coverage(r) = &mut row.body {
                match variant {
                    0 => r.last_event.event_ordinal -= 1,
                    1 => r.last_event.accepted_daa += 1,
                    _ => r.last_event.accepted_blue_score += 1,
                }
            }
        }
        assert!(check(&rows).is_err(), "empty coverage variant {variant}");
    }
}

#[test]
fn progressed_coverage_may_finish_with_a_packet_but_cannot_precede_any_retained_packet() {
    let baseline = ordered_rows();
    assert!(check(&baseline).is_ok()); // Packet and NodeFinish may be one event.
    for variant in 0..3 {
        let mut rows = baseline.clone();
        for row in &mut rows {
            if let NativeRowBody::Coverage(r) = &mut row.body {
                match variant {
                    0 => r.last_event.event_ordinal -= 1,
                    1 => r.last_event.accepted_daa += 1,
                    _ => r.last_event.accepted_blue_score += 1,
                }
            }
        }
        assert!(check(&rows).is_err(), "progressed coverage variant {variant}");
    }
    let mut rows = baseline.clone();
    for row in &mut rows {
        if let NativeRowBody::PacketReceipt(r) = &mut row.body {
            if r.first_relation == 1 {
                r.origin = event(31);
            }
        }
    }
    assert!(check(&rows).is_err(), "a second later packet cannot be hidden by the first packet");
    for row in &mut rows {
        if let NativeRowBody::Coverage(r) = &mut row.body {
            r.last_event = event(31);
        }
    }
    assert!(check(&rows).is_ok());
}

#[test]
fn service_response_and_its_settlement_can_be_atomic_but_never_reverse_same_block_order() {
    let baseline = ordered_rows();
    assert!(check(&baseline).is_ok()); // response == settlement is permitted.
    for variant in 0..5 {
        let mut rows = baseline.clone();
        for row in &mut rows {
            if let NativeRowBody::ServiceReceipt(r) = &mut row.body {
                let settlement = &mut r.settlement.as_mut().unwrap().origin;
                match variant {
                    0 => settlement.event_ordinal -= 1,
                    1 => settlement.accepted_daa += 1,
                    2 => settlement.accepted_blue_score += 1,
                    3 => settlement.event_ordinal += 1,
                    _ => {
                        settlement.accepting_block = NativeHash64([102; 64]);
                        settlement.accepted_tx_ordinal = 0;
                        settlement.event_ordinal = 0;
                        settlement.accepted_daa = 1;
                        settlement.accepted_blue_score = 1;
                    }
                }
            }
        }
        assert_eq!(check(&rows).is_ok(), variant >= 3, "settlement variant {variant}");
    }
}
