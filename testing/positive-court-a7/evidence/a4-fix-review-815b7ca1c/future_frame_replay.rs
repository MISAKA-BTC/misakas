// NOT EXECUTED. Add inside positive_court_a4_integration.rs so its existing
// private fixture/helpers are in scope. The relay copies one valid signature;
// it does not sign another packet. Initial refusal cost policy may be zero or
// nonzero; identical refused copies must not multiply the Case charge.
#[test]
fn copied_future_packet_does_not_multiply_case_cost_and_can_later_arrive_in_order() {
    let f = fixture();
    let (result, service) = court_result(&f, false);
    let mut court = court(result.clone(), service);
    let (_, descriptor, frames) = open(&mut court, &f, &result, 0);
    assert!(frames.len() > 1);
    let future = EvidenceFrame::decode(&frames[1], descriptor.max_part_bytes).unwrap();
    let signature = carrier_signature(&future);
    assert!(court.submit_frame(future, &signature, 3).is_err());
    let after_first = (
        court.attempts(),
        court.cursor(),
        court.charged_wire_bytes(),
        court.charged_elements(),
        court.charged_terms(),
    );
    for _ in 0..4 {
        let future = EvidenceFrame::decode(&frames[1], descriptor.max_part_bytes).unwrap();
        let _ = court.submit_frame(future, &signature, 3);
        assert_eq!(
            (
                court.attempts(),
                court.cursor(),
                court.charged_wire_bytes(),
                court.charged_elements(),
                court.charged_terms(),
            ),
            after_first
        );
        assert_eq!(court.cursor(), 0);
        assert_eq!(court.status(), ClaimStatus::Pending);
    }
    for bytes in &frames {
        send(&mut court, bytes, descriptor.max_part_bytes, 4);
    }
    assert_eq!(court.finish(5).unwrap().terminal_kind(), TerminalKindV2::PositivePassed);
}
