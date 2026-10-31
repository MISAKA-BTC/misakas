//! Literal receipt lower bounds, never replay, payment or credit authority.
use super::super::tests::{all_rows, d, encoded_rows, material_at, origin_at, signed_bundle_for_relations};
use super::*;
use misaka_palw_positive_contract::messages::WitnessStatus;

fn check(rows: &[NativeRowValue]) -> Result<PositiveRestoreReportV2, PositiveRowErrorV2> {
    let aux = encoded_rows(rows).into_iter().map(|(table, key, value)| ((table, key), value)).collect();
    validate_positive_aux_restore_v2(&aux)
}

fn case_mut(rows: &mut [NativeRowValue]) -> &mut CaseRow {
    rows.iter_mut().find_map(|r| if let NativeRowBody::Case(c) = &mut r.body { Some(c) } else { None }).unwrap()
}
fn coverage_mut(rows: &mut [NativeRowValue]) -> &mut CoverageRow {
    rows.iter_mut().find_map(|r| if let NativeRowBody::Coverage(c) = &mut r.body { Some(c) } else { None }).unwrap()
}
fn service_totals(rows: &[NativeRowValue]) -> (u64, u64) {
    rows.iter().fold((0, 0), |(bytes, work), r| match &r.body {
        NativeRowBody::ServiceReceipt(s) => (bytes + s.bytes, work + s.additional_work),
        _ => (bytes, work),
    })
}
fn packet_totals(rows: &[NativeRowValue]) -> (u64, u64) {
    rows.iter().fold((0, 0), |(bytes, count), r| match &r.body {
        NativeRowBody::PacketReceipt(p) => (bytes + p.signed_bytes - SIGNATURE_BYTES as u64, count + 1),
        _ => (bytes, count),
    })
}

fn pending_packet_rows(mandatory: u64, retained: u64) -> Vec<NativeRowValue> {
    let (index, result, mut case, funding, witness, mut coverage) = signed_bundle_for_relations(mandatory);
    let final_event = 6u32.checked_add(u32::try_from(retained).unwrap()).unwrap();
    case.status = ClaimStatus::Pending;
    case.witness_status = WitnessStatus::Checking;
    case.miner_data = MinerDataStatus::NotRequested;
    case.last_event = origin_at(final_event);
    coverage.state = CoverageState::Checking;
    coverage.last_event = case.last_event;
    coverage.frame_bytes = 1373u64.checked_mul(retained).unwrap();
    coverage.signatures = retained;
    let mut rows: Vec<_> = [
        NativeRowBody::ClaimIndex(index),
        NativeRowBody::Result(result),
        NativeRowBody::Case(case),
        NativeRowBody::Funding(funding),
        NativeRowBody::Witness(witness.clone()),
        NativeRowBody::Coverage(coverage),
        NativeRowBody::JournalHead(JournalHeadRow {
            case: case.case,
            sequence: 2,
            previous: d(95),
            head: d(96),
            history: AcceptedMaterial { origin: case.last_event, ..material_at(19) },
        }),
    ]
    .into_iter()
    .map(|body| NativeRowValue::new(body).unwrap())
    .collect();
    for first_relation in 0..retained {
        let event = 7u32.checked_add(u32::try_from(first_relation).unwrap()).unwrap();
        let mut frame = [77; 64];
        frame[..8].copy_from_slice(&first_relation.to_le_bytes());
        rows.push(
            NativeRowValue::new(NativeRowBody::PacketReceipt(PacketReceiptRow {
                witness: witness.witness.id(),
                first_relation,
                frame: EvidencePartId(Digest(frame)),
                author: witness.witness.author,
                signed_bytes: 6000,
                origin: origin_at(event),
            }))
            .unwrap(),
        );
    }
    rows
}

#[test]
fn each_case_usage_counter_covers_every_literal_service_receipt() {
    let baseline = all_rows();
    check(&baseline).unwrap();
    let (bytes, work) = service_totals(&baseline);
    assert!(bytes > 0 && work > 0);
    for counter in 0..3 {
        let mut rows = baseline.clone();
        let usage = &mut case_mut(&mut rows).usage;
        match counter {
            0 => usage.unique_source_bytes = bytes - 1,
            1 => usage.transmitted_bytes = bytes - 1,
            _ => usage.additional_work = work - 1,
        }
        assert!(check(&rows).is_err(), "understated service counter {counter}");
    }
}

#[test]
fn equal_unit_numbers_from_distinct_sources_cannot_share_one_usage_charge() {
    let mut rows = all_rows();
    let source = Digest([103; 64]);
    let mut union = rows.iter().find(|r| matches!(r.body, NativeRowBody::SourceUnion(_))).unwrap().clone();
    if let NativeRowBody::SourceUnion(u) = &mut union.body {
        u.source = source;
        u.requested_units = 1;
        u.completed_units = 1;
    }
    let mut receipt = rows.iter().find(|r| matches!(r.body, NativeRowBody::ServiceReceipt(_))).unwrap().clone();
    if let NativeRowBody::ServiceReceipt(r) = &mut receipt.body {
        r.source = source;
    }
    rows.extend([union, receipt]);
    assert!(check(&rows).is_err());
    let (bytes, work) = service_totals(&rows);
    let usage = &mut case_mut(&mut rows).usage;
    usage.unique_source_bytes = bytes;
    usage.transmitted_bytes = bytes;
    usage.additional_work = work;
    check(&rows).unwrap();
}

#[test]
fn coverage_bytes_and_signature_count_cover_the_literal_packet_set() {
    let baseline = all_rows();
    let (bytes, count) = packet_totals(&baseline);
    assert_eq!(count, 10);
    assert_eq!(coverage_mut(&mut baseline.clone()).next_relation, count);
    for lower_bytes in [false, true] {
        let mut rows = baseline.clone();
        let coverage = coverage_mut(&mut rows);
        if lower_bytes {
            coverage.frame_bytes = bytes - 1;
        } else {
            coverage.signatures = count - 1;
        }
        assert!(check(&rows).is_err(), "understated packet bytes={lower_bytes}");
    }
}

#[test]
fn progressed_cursor_requires_all_distinct_relation_keys_without_gaps_or_excess() {
    let baseline = all_rows();
    let mut missing_one = baseline.clone();
    missing_one.retain(|r| !matches!(&r.body, NativeRowBody::PacketReceipt(p) if p.first_relation == 5));
    assert!(check(&missing_one).is_err());
    let mut missing_all = baseline.clone();
    missing_all.retain(|r| !matches!(r.body, NativeRowBody::PacketReceipt(_)));
    assert!(check(&missing_all).is_err(), "Matched cache cannot survive without its packet rows");
    let mut displaced = baseline.clone();
    for row in &mut displaced {
        if let NativeRowBody::PacketReceipt(p) = &mut row.body {
            if p.first_relation == 5 {
                p.first_relation = 10;
            }
        }
    }
    assert!(check(&displaced).is_err(), "count alone cannot hide a gap plus out-of-cursor key");
    let mut ahead = baseline;
    case_mut(&mut ahead).status = ClaimStatus::Pending;
    let coverage = coverage_mut(&mut ahead);
    coverage.state = CoverageState::Checking;
    coverage.next_relation = 9;
    assert!(check(&ahead).is_err(), "every retained packet must be below next_relation");
}

#[test]
fn retry_and_failed_work_can_make_counters_larger_than_the_retained_success_receipts() {
    let mut rows = all_rows();
    for row in &mut rows {
        match &mut row.body {
            NativeRowBody::ServiceReceipt(r) => {
                r.bytes -= 1;
                r.additional_work = 0;
            }
            NativeRowBody::PacketReceipt(r) => r.signed_bytes -= 1,
            NativeRowBody::Coverage(r) => r.signatures += 1,
            _ => (),
        }
    }
    check(&rows).unwrap();
    let (bytes, work) = service_totals(&rows);
    let usage = case_mut(&mut rows).usage;
    assert!(usage.unique_source_bytes > bytes && usage.transmitted_bytes > bytes && usage.additional_work > work);
    let (bytes, count) = packet_totals(&rows);
    let coverage = coverage_mut(&mut rows);
    assert!(coverage.frame_bytes > bytes && coverage.signatures > count);
}

#[test]
fn little_endian_service_unit_keys_are_summed_without_a_numeric_sort_assumption() {
    let mut rows = all_rows();
    let mut template = rows.iter().find(|r| matches!(r.body, NativeRowBody::ServiceReceipt(_))).unwrap().clone();
    rows.retain(|r| !matches!(r.body, NativeRowBody::ServiceReceipt(_)));
    for row in &mut rows {
        if let NativeRowBody::SourceUnion(u) = &mut row.body {
            u.source_units = 512;
            u.completed_units = 4;
        }
    }
    for unit in [0, 4, 256, 257] {
        if let NativeRowBody::ServiceReceipt(r) = &mut template.body {
            r.unit = unit;
            r.bytes = 1024;
            r.additional_work = 0;
        }
        rows.push(template.clone());
    }
    let mut keys: Vec<_> = rows
        .iter()
        .filter_map(|r| match &r.body {
            NativeRowBody::ServiceReceipt(s) => Some((r.body.key().encode(), s.unit)),
            _ => None,
        })
        .collect();
    keys.sort();
    assert_eq!(keys.iter().map(|(_, unit)| *unit).collect::<Vec<_>>(), [0, 256, 257, 4]);
    check(&rows).unwrap();
    case_mut(&mut rows).usage.unique_source_bytes = 4095;
    assert!(check(&rows).is_err());
}

#[test]
fn complete_packet_cursor_is_independent_of_little_endian_key_iteration_order() {
    let mut rows = pending_packet_rows(257, 257);
    let mut keys: Vec<_> = rows
        .iter()
        .filter_map(|r| match &r.body {
            NativeRowBody::PacketReceipt(p) => Some((r.body.key().encode(), p.first_relation)),
            _ => None,
        })
        .collect();
    keys.sort();
    assert_eq!(keys.iter().take(4).map(|(_, relation)| *relation).collect::<Vec<_>>(), [0, 256, 1, 2]);
    check(&rows).unwrap();
    rows.retain(|r| !matches!(&r.body, NativeRowBody::PacketReceipt(p) if p.first_relation == 256));
    assert!(check(&rows).is_err());
}

#[test]
fn hostile_maximum_cursor_is_rejected_using_present_rows_without_allocating_or_looping_over_the_cursor() {
    let rows = pending_packet_rows(u64::MAX, 1);
    // The claimed range is enormous, but the signed fixture has eight rows.
    // Restore must scan those literal rows and compare the count; it must never
    // enumerate 0..next_relation or reserve a cursor-sized bitmap/index.
    assert_eq!(rows.len(), 8);
    assert!(check(&rows).is_err());
}

#[test]
fn receipt_byte_work_and_packet_frame_sums_reject_overflow_instead_of_wrapping() {
    let baseline = all_rows();
    for overflow_work in [false, true] {
        let mut rows = baseline.clone();
        let mut second = rows.iter().find(|r| matches!(r.body, NativeRowBody::ServiceReceipt(_))).unwrap().clone();
        for row in &mut rows {
            match &mut row.body {
                NativeRowBody::ServiceReceipt(r) => {
                    r.bytes = if overflow_work { 2048 } else { u64::MAX };
                    r.additional_work = if overflow_work { u64::MAX } else { 0 };
                }
                NativeRowBody::SourceUnion(u) => u.completed_units = 2,
                _ => (),
            }
        }
        if let NativeRowBody::ServiceReceipt(r) = &mut second.body {
            r.unit = 5;
            r.bytes = if overflow_work { 2048 } else { 1 };
            r.additional_work = u64::from(overflow_work);
        }
        rows.push(second);
        assert!(check(&rows).is_err(), "service overflow work={overflow_work}");
    }
    let mut rows = baseline;
    for row in &mut rows {
        if let NativeRowBody::PacketReceipt(p) = &mut row.body {
            p.signed_bytes = u64::MAX;
        }
    }
    coverage_mut(&mut rows).frame_bytes = u64::MAX;
    assert!(check(&rows).is_err(), "frame subtraction then checked summation must not wrap");
}
