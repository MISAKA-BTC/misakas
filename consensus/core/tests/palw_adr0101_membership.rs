//! ADR-0101 — a membership is proven by the chain and served by anyone, and a Position never moves
//! between holders. The invariants that are consensus-core's to hold.

/// **Decision 5 — a Position moves only between a holder and the curve** (settled by the operator
/// 2026-09-10: a Position has no payment or settlement role, so it has no transfer). The fold has
/// exactly two writers of a holding: the buy credits its buyer from the curve, the sell debits its
/// seller to the curve. The reward's buyback (ADR-0091) retires units into the market row and
/// credits no holder; a line's owner transfer (ADR-0088) moves no Position. A third writer — a
/// transfer, a payment, a "pay with Positions" — fails this test, and the answer to that failure
/// is an ADR that supersedes ADR-0101 Decision 5 by name, not an edit here.
#[test]
fn a_position_moves_only_between_a_holder_and_the_curve() {
    let source = include_str!("../src/palw_state_v2.rs");
    let lines: Vec<&str> = source.lines().collect();
    let enclosing_fn = |at: usize| -> String {
        lines[..at]
            .iter()
            .rev()
            .find_map(|l| {
                let t = l.trim_start();
                let rest = t.strip_prefix("pub fn ").or_else(|| t.strip_prefix("fn ")).or_else(|| t.strip_prefix("pub(crate) fn "))?;
                Some(rest.split(['(', '<']).next().unwrap_or("").to_string())
            })
            .unwrap_or_default()
    };
    let writers: Vec<String> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| {
            l.contains("write_model_position(") && !l.trim_start().starts_with("fn ") && !l.trim_start().starts_with("//")
        })
        .map(|(i, _)| enclosing_fn(i))
        .collect();
    assert_eq!(
        writers,
        vec!["model_buy_v1".to_string(), "model_sell_v1".to_string()],
        "a holding is written by the buy and the sell and by nothing else"
    );
    // The object enum names no Position transfer; the one "Transferred" object moves a LINE's
    // ownership, and its arm writes no holding (the count above).
    let transfer_objects: Vec<&str> = lines
        .iter()
        .filter_map(|l| {
            let t = l.trim_start();
            let name = t.strip_suffix(" {")?;
            (l.starts_with("    ")
                && !l.starts_with("     ")
                && name.contains("Transfer")
                && name.chars().all(|c| c.is_ascii_alphanumeric()))
            .then_some(name)
        })
        .collect();
    assert_eq!(transfer_objects, vec!["ModelLineOwnerTransferred"], "no object moves a Position from one holder to another");
}

/// **Decision 3 — the check a client runs holds for a descriptor built by the SDK's transport
/// form**: JSON in, the same id out, and the check's verdict turns on the declared grants, the
/// chain's roots and the line's keys — the unit tests in `palw_service_descriptor_v1` hold the
/// refusals one by one; this holds that the pieces compose over the transport.
#[test]
fn a_descriptor_survives_its_transport_and_keeps_its_id() {
    use kaspa_consensus_core::palw_model_benefits_v1::grant;
    use kaspa_consensus_core::palw_service_descriptor_v1::{
        PalwLineServiceFactsV1, PalwServiceDescriptorV1, PalwServiceProviderKindV1, palw_service_descriptor_check_v1,
        palw_service_descriptor_id_v1,
    };
    use kaspa_hashes::Hash64;
    let domain = Hash64::from_u64_word(0xD0);
    let d = PalwServiceDescriptorV1 {
        version: 1,
        line_id: Hash64::from_u64_word(7),
        grants: grant::PRIORITY_INFERENCE,
        roots: vec![Hash64::from_u64_word(100)],
        endpoints: vec!["https://provider.example/v1".into()],
        valid_from_daa: 1,
        expires_daa: 10,
        provider_pubkey: b"k".to_vec(),
        signature: b"sig".to_vec(),
    };
    let json = serde_json::to_string_pretty(&d).unwrap();
    let back: PalwServiceDescriptorV1 = serde_json::from_str(&json).unwrap();
    assert_eq!(palw_service_descriptor_id_v1(domain, &back), palw_service_descriptor_id_v1(domain, &d));
    let facts = PalwLineServiceFactsV1 {
        line_id: d.line_id,
        declared_grants: grant::PRIORITY_INFERENCE,
        roots: vec![Hash64::from_u64_word(100)],
        origin_pubkeys: vec![],
        now_daa: 5,
    };
    let expect_id = palw_service_descriptor_id_v1(domain, &d);
    let verdict = palw_service_descriptor_check_v1(domain, &back, &facts, |pk, msg, sig, _| {
        pk == b"k" && sig == b"sig" && msg == expect_id.as_byte_slice()
    });
    assert_eq!(verdict, Ok(PalwServiceProviderKindV1::Open), "a stranger serving by root, over the transport");
}
