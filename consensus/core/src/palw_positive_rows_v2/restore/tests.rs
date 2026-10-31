use super::super::tests::{all_rows, d, encoded_rows};
use super::*;

fn rows_map(rows: &[NativeRowValue]) -> PositiveAuxRowsV2 {
    encoded_rows(rows).into_iter().map(|(table, key, value)| ((table, key), value)).collect()
}
fn check(rows: &[NativeRowValue]) -> Result<PositiveRestoreReportV2, PositiveRowErrorV2> {
    validate_positive_aux_restore_v2(&rows_map(rows))
}
fn update(rows: &mut [NativeRowValue], mut modify: impl FnMut(&mut NativeRowBody)) {
    for row in rows {
        modify(&mut row.body);
    }
}

#[test]
fn complete_signed_bundle_restores_only_a_structural_diagnostic() {
    let rows = all_rows();
    let report = check(&rows).unwrap();
    assert_eq!(report.rows, 10);
    assert_eq!(report.cases, 1);
    assert_eq!(report.witnesses, 1);
    assert_eq!(report.signature_checks, 3);
    assert_eq!(report.witness_attempt_entries, 1);
    assert!(report.reference_reads > report.rows);
    assert!(report.decoded_rows > report.rows);
    // Matched and budget summaries remain caches. No LocalCheckedTerminal,
    // spend/discount authority or credit constructor is returned by this API.
    assert_eq!(validate_positive_aux_restore_v2(&BTreeMap::new()), Ok(PositiveRestoreReportV2::default()));
}

#[test]
fn every_admission_row_and_positive_witness_link_is_required() {
    let all = all_rows();
    for kind in [0, 1, 2, 3, 4, 5, 6, 7, 9] {
        let mut rows = all.clone();
        rows.remove(kind);
        assert!(check(&rows).is_err(), "removing kind {kind} must not leave a valid native bundle");
    }
    // No individual PacketReceipt is required merely by a saved Matched cache.
    // The native accepted-history replay must recover actual signed frames.
    let mut no_packets = all;
    no_packets.remove(8);
    assert!(check(&no_packets).is_ok());
}

#[test]
fn duplicate_and_malformed_rows_are_rejected_before_typed_vec_collection() {
    let rows = all_rows();
    let mut encoded = encoded_rows(&rows);
    validate_positive_aux_uncollected_v2(&encoded).unwrap();
    encoded.push(encoded[0].clone());
    assert_eq!(validate_positive_aux_uncollected_v2(&encoded), Err(PositiveRowErrorV2::InconsistentLinks));
    let mut encoded = encoded_rows(&rows);
    encoded[0].1[0] = 1;
    assert!(validate_positive_aux_uncollected_v2(&encoded).is_err());
    let mut map = rows_map(&rows);
    let (_, key, value) = encoded_rows(&rows).remove(0);
    let mut tail = value.clone();
    tail.push(0);
    map.insert((KERNEL_AUX_TABLE, key.clone()), tail);
    assert!(validate_positive_aux_restore_v2(&map).is_err());
    let mut map = rows_map(&rows);
    let mut unknown = key;
    unknown[4] = 255;
    map.insert((KERNEL_AUX_TABLE, unknown), value);
    assert!(validate_positive_aux_restore_v2(&map).is_err());
}

#[test]
fn orphan_rows_of_all_reference_kinds_are_rejected() {
    for kind in 1..10 {
        let mut rows = all_rows();
        let mut orphan = rows[kind].clone();
        match &mut orphan.body {
            NativeRowBody::ClaimIndex(r) => r.claim = ClaimId(d(91)),
            NativeRowBody::Case(r) => {
                r.case = CaseId(d(91));
                r.service.case = r.case;
            }
            NativeRowBody::Witness(r) => r.witness.case = CaseId(d(91)),
            NativeRowBody::Coverage(r) => r.witness = WitnessId(d(91)),
            NativeRowBody::SourceUnion(r) => r.case = CaseId(d(91)),
            NativeRowBody::ServiceReceipt(r) => r.case = CaseId(d(91)),
            NativeRowBody::Funding(r) => r.consent.case = CaseId(d(91)),
            NativeRowBody::PacketReceipt(r) => r.witness = WitnessId(d(91)),
            NativeRowBody::JournalHead(r) => r.case = CaseId(d(91)),
            NativeRowBody::Result(_) => unreachable!(),
        }
        rows.push(orphan);
        assert!(check(&rows).is_err(), "orphan kind {kind} must fail");
    }
}

#[test]
fn native_initial_journal_is_required_and_uses_its_own_sequence_convention() {
    for variant in 0..4 {
        let mut rows = all_rows();
        update(&mut rows, |body| match body {
            NativeRowBody::JournalHead(r) => match variant {
                0 => r.sequence = 0,
                1 => r.previous = d(9),
                2 => r.history.origin.accepted_daa += 1,
                _ => {
                    r.history.origin.accepted_daa += 1;
                }
            },
            NativeRowBody::Case(r) if variant == 3 => r.last_event.accepted_daa += 1,
            _ => (),
        });
        assert!(check(&rows).is_err());
    }
    let mut rows = all_rows();
    update(&mut rows, |body| match body {
        NativeRowBody::JournalHead(r) => {
            r.sequence = 2;
            r.previous = d(9);
            r.history.origin.accepted_daa += 1;
        }
        NativeRowBody::Case(r) => r.last_event.accepted_daa += 1,
        _ => (),
    });
    assert!(check(&rows).is_ok()); // Still no accepted-origin capability.
}

#[test]
fn union_summaries_must_equal_retained_canonical_once_only_receipts() {
    let mut rows = all_rows();
    update(&mut rows, |body| {
        if let NativeRowBody::SourceUnion(r) = body {
            r.completed_units = 2;
        }
    });
    assert!(check(&rows).is_err());
    let mut rows = all_rows();
    update(&mut rows, |body| {
        if let NativeRowBody::Case(r) = body {
            r.miner_data = MinerDataStatus::Complete;
        }
    });
    assert!(check(&rows).is_err());
    let mut rows = all_rows();
    update(&mut rows, |body| {
        if let NativeRowBody::ServiceReceipt(r) = body {
            r.result = ResultId(d(91));
        }
    });
    assert!(check(&rows).is_err());
    let mut rows = all_rows();
    rows.remove(6);
    update(&mut rows, |body| {
        if let NativeRowBody::SourceUnion(r) = body {
            r.completed_units = 0;
        }
    });
    assert!(check(&rows).is_ok());
    let mut rows = all_rows();
    let mut receipt = rows[6].clone();
    if let NativeRowBody::ServiceReceipt(r) = &mut receipt.body {
        r.unit = 8;
    }
    rows.push(receipt);
    update(&mut rows, |body| {
        if let NativeRowBody::SourceUnion(r) = body {
            r.completed_units = 2;
        }
    });
    assert!(check(&rows).is_ok());
}

#[test]
fn packet_author_and_canonical_relation_bound_are_checked() {
    let mut rows = all_rows();
    update(&mut rows, |body| {
        if let NativeRowBody::PacketReceipt(r) = body {
            r.author = PrincipalId(d(91));
        }
    });
    assert!(check(&rows).is_err());
    let mut rows = all_rows();
    update(&mut rows, |body| {
        if let NativeRowBody::PacketReceipt(r) = body {
            r.first_relation = 10;
        }
    });
    assert!(check(&rows).is_err());
}

fn add_panel_attempt(rows: &mut Vec<NativeRowValue>, attempt: u32) {
    let mut witness = rows[3].clone();
    let mut coverage = rows[4].clone();
    let NativeRowBody::Witness(w) = &mut witness.body else { unreachable!() };
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([4; 32]);
    w.witness.author = principal(key.verification_key.as_ref());
    w.witness.attempt = attempt;
    w.panel_key = *key.verification_key.as_ref();
    w.panel_signature =
        *libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, &w.witness.id().0.0, WITNESS_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
    if let NativeRowBody::Coverage(c) = &mut coverage.body {
        c.witness = w.witness.id();
    }
    rows.push(witness);
    rows.push(coverage);
    update(rows, |body| {
        if let NativeRowBody::Case(c) = body {
            c.usage.witness_attempts = 2;
        }
    });
}

#[test]
fn panel_key_replacement_cannot_reuse_global_attempt_ordinal() {
    let mut duplicate = all_rows();
    add_panel_attempt(&mut duplicate, 0);
    assert_eq!(check(&duplicate), Err(PositiveRowErrorV2::InconsistentLinks));
    let mut next = all_rows();
    add_panel_attempt(&mut next, 1);
    assert!(check(&next).is_ok());
    let mut rows = all_rows();
    update(&mut rows, |body| {
        if let NativeRowBody::Case(c) = body {
            c.usage.witness_attempts = 0;
        }
    });
    assert!(check(&rows).is_err());
}

#[test]
fn saved_pass_cannot_point_to_foreign_witness_and_signature_failure_is_not_absence() {
    let mut rows = all_rows();
    update(&mut rows, |body| {
        if let NativeRowBody::Case(c) = body {
            c.positive_witness = Some(WitnessId(d(91)));
        }
    });
    assert!(check(&rows).is_err());
    let mut rows = all_rows();
    update(&mut rows, |body| {
        if let NativeRowBody::Result(r) = body {
            r.miner_signature[0] ^= 1;
        }
    });
    assert!(matches!(check(&rows), Err(PositiveRowErrorV2::Contract(ContractError::Signature))));
    let mut rows = all_rows();
    update(&mut rows, |body| {
        if let NativeRowBody::Witness(r) = body {
            r.panel_signature[0] ^= 1;
        }
    });
    assert!(matches!(check(&rows), Err(PositiveRowErrorV2::Contract(ContractError::Signature))));
}
