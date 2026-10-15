//! **Registry completeness for the native aux scanner.** The set of aux tables is discovered from the crate's SOURCE (not from
//! the generator's spec): every `u8` table constant at or above the first aux table, every constant a `write_kernel_row(…)` call
//! names as its table (directly, through a local or alias, or in the `(table, key, row)` tuples a loop writes). Each one must
//! reach an explicit arm of `scan_native_aux_row_v1`: an empty probe row may fail as malformed, never as "not in the supported
//! shape inventory". A new table any fold populates therefore fails here until its codec is added
//! (`scripts/drills/rfc06_native_aux_spec.py`, then `scripts/drills/gen_rfc06_native_wire.py`).
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("readable source directory") {
        let path = entry.expect("readable directory entry").path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Index just past the bracket closing the one opened right before `start`.
fn closing(s: &[u8], start: usize) -> usize {
    let mut depth = 1usize;
    let mut i = start;
    while i < s.len() && depth > 0 {
        match s[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    i
}

/// Top-level comma-separated parts of a bracket body.
fn top_level_parts(body: &str) -> Vec<&str> {
    let (mut parts, mut depth, mut start) = (Vec::new(), 0i32, 0usize);
    for (i, c) in body.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&body[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&body[start..]);
    parts
}

fn upper_tokens(text: &str) -> Vec<&str> {
    text.split(|c: char| !is_ident(c))
        .filter(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'))
        .collect()
}

/// `const NAME: u8 = N;` (literal) and `const NAME: u8 = path::OTHER;` (alias, resolved after).
fn table_constants(sources: &[(PathBuf, String)]) -> BTreeMap<String, u8> {
    let (mut literal, mut alias) = (BTreeMap::new(), Vec::new());
    for (_, text) in sources {
        for line in text.lines() {
            let Some(at) = line.find("const ") else { continue };
            let rest = &line[at + 6..];
            let Some((name, rest)) = rest.split_once(':') else { continue };
            let Some(value) = rest.trim_start().strip_prefix("u8") else { continue };
            let Some(value) = value.trim_start().strip_prefix('=') else { continue };
            let value = value.trim().trim_end_matches(';').trim();
            let name = name.trim().to_string();
            if let Ok(n) = value.parse::<u8>() {
                literal.entry(name).or_insert(n);
            } else if let Some(target) = value.rsplit("::").next() {
                alias.push((name, target.to_string()));
            }
        }
    }
    for (name, target) in alias {
        if let Some(n) = literal.get(&target).copied() {
            literal.entry(name).or_insert(n);
        }
    }
    literal
}

/// `fn put(b, table: u8, …) { … write_kernel_row(table, …) … }`: helper name → the parameter position that carries the table.
fn writer_helpers(sources: &[(PathBuf, String)]) -> BTreeMap<String, usize> {
    let mut helpers = BTreeMap::new();
    for (_, text) in sources {
        let bytes = text.as_bytes();
        let mut from = 0;
        while let Some(at) = text[from..].find("fn ") {
            let start = from + at + 3;
            from = start;
            if start > 3 && is_ident(text[..start - 3].chars().last().unwrap_or(' ')) {
                continue;
            }
            let name: String = text[start..].chars().take_while(|c| is_ident(*c)).collect();
            let Some(open) = text[start..].find('(').map(|i| start + i + 1) else { break };
            if name.is_empty() || open - start > name.len() + 160 {
                continue;
            }
            let close = closing(bytes, open);
            let Some(body) = text[close..].find('{').map(|i| close + i + 1) else { continue };
            let body_end = closing(bytes, body);
            let body_text: String = text[body..body_end].chars().filter(|c| !c.is_whitespace()).collect();
            for (index, param) in top_level_parts(&text[open..close.saturating_sub(1)]).into_iter().enumerate() {
                let compact: String = param.chars().filter(|c| !c.is_whitespace()).collect();
                if let Some(pname) = compact.strip_suffix(":u8") {
                    if body_text.contains(&format!("write_kernel_row({pname},")) {
                        helpers.insert(name.clone(), index);
                    }
                }
            }
        }
    }
    helpers
}

fn written_tables(sources: &[(PathBuf, String)], consts: &BTreeMap<String, u8>) -> BTreeSet<u8> {
    let helpers = writer_helpers(sources);
    let mut tables = BTreeSet::new();
    let mut note = |text: &str| {
        for token in upper_tokens(text) {
            if let Some(n) = consts.get(token) {
                tables.insert(*n);
            }
        }
    };
    for (_, text) in sources {
        let bytes = text.as_bytes();
        let mut from = 0;
        while let Some(at) = text[from..].find("write_kernel_row(") {
            let open = from + at + "write_kernel_row(".len();
            let close = closing(bytes, open);
            let first = top_level_parts(&text[open..close.saturating_sub(1)])[0].trim();
            note(first);
            // A local (`table`, `t`): the expression it was bound to, in the text before the call.
            let local = first.trim_start_matches('*');
            if !local.is_empty() && local.chars().all(is_ident) && local.chars().any(|c| c.is_ascii_lowercase()) {
                let before = &text[..open];
                if let Some(bind) = before.rfind(&format!("let {local}")) {
                    let end = before[bind..].find(';').map_or(before.len(), |e| bind + e);
                    note(&before[bind..end]);
                }
            }
            from = close;
        }
        // Tables passed to a writer helper.
        for (helper, index) in &helpers {
            let call = format!("{helper}(");
            let mut from = 0;
            while let Some(at) = text[from..].find(&call) {
                let start = from + at;
                let open = start + call.len();
                from = open;
                if start > 0 && is_ident(text[..start].chars().last().unwrap_or(' ')) || text[..start].ends_with("fn ") {
                    continue;
                }
                let close = closing(bytes, open);
                if let Some(arg) = top_level_parts(&text[open..close.saturating_sub(1)]).get(*index) {
                    note(arg);
                }
            }
        }
        // `for (table, key, row) in [(T, …), …] { … write_kernel_row(table, …) … }` and fixed-table key/row loops.
        let mut from = 0;
        while let Some(at) = text[from..].find("for (") {
            let start = from + at;
            let Some(array_at) = text[start..].find(" in [").map(|i| start + i + " in [".len()) else { break };
            if array_at - start > 120 {
                from = start + 5;
                continue;
            }
            let array_end = closing(bytes, array_at);
            let body_end = text[array_end..].find('{').map_or(array_end, |b| closing(bytes, array_end + b + 1));
            if text[array_end..body_end].contains("write_kernel_row(") {
                for tuple in top_level_parts(&text[array_at..array_end.saturating_sub(1)]) {
                    let tuple = tuple.trim();
                    if let Some(inner) = tuple.strip_prefix('(') {
                        note(top_level_parts(inner.trim_end_matches(')'))[0]);
                    }
                }
            }
            from = array_end.max(start + 5);
        }
    }
    tables
}

#[test]
fn every_aux_table_a_fold_can_populate_has_an_explicit_scanner_arm() {
    let mut files = Vec::new();
    rust_sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    let sources: Vec<(PathBuf, String)> =
        files.into_iter().map(|p| (p.clone(), std::fs::read_to_string(&p).expect("readable source"))).collect();
    let consts = table_constants(&sources);
    let first_aux = crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_FIRST_AUX_TABLE_V1;
    let mut tables = written_tables(&sources, &consts);
    tables.extend(consts.iter().filter(|(name, _)| name.contains("TABLE")).map(|(_, n)| *n));
    tables.retain(|t| *t >= first_aux);
    // The discovery itself must see the families this registry exists for (a parser regression cannot pass silently).
    for expected in [41u8, 46, 64, 87, 92, 99, 106, 112, 113, 120, 130] {
        assert!(tables.contains(&expected), "source discovery lost aux table {expected}: {tables:?}");
    }
    for table in tables {
        let mut empty = NativeWireReaderV1::new(&[][..], 0);
        if let Err(error) = super::wire_kernel_aux::scan_native_aux_row_v1(table, &[], &mut empty) {
            assert_ne!(
                error.kind(),
                std::io::ErrorKind::Unsupported,
                "aux table {table} is populated by a fold but has no explicit scanner arm ({error})"
            );
        }
    }
}

#[test]
fn ambiguous_and_raw_rows_are_bounded_and_never_under_counted() {
    // A raw row: its bytes are accounted once more, its declared bound is enforced (table 91, a 64-byte job key).
    let raw = vec![7u8; 100];
    let key = [1u8; 64];
    let mut reader = NativeWireReaderV1::new(raw.as_slice(), raw.len() as u64);
    let shape = super::wire_kernel_aux::scan_native_aux_row_v1(91, &key, &mut reader).unwrap();
    assert_eq!(reader.remaining(), 0);
    assert!(shape.heap >= 100 && shape.byte_entries == 100);
    // An unallocated key shape inside a covered table fails closed as malformed.
    let mut reader = NativeWireReaderV1::new(raw.as_slice(), raw.len() as u64);
    let error = super::wire_kernel_aux::scan_native_aux_row_v1(91, &[9u8; 7], &mut reader).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    // Table 105's (14, job, ordinal) shape: a 64-byte row is the v11 demand id, a 65-byte row the (16, source16 pending) pair.
    let mut key = vec![14u8];
    key.extend_from_slice(&[2u8; 64]);
    key.extend_from_slice(&3u32.to_le_bytes());
    for row in [vec![5u8; 64], vec![5u8; 65]] {
        let mut reader = NativeWireReaderV1::new(row.as_slice(), row.len() as u64);
        super::wire_kernel_aux::scan_native_aux_row_v1(105, &key, &mut reader).unwrap();
        assert_eq!(reader.remaining(), 0);
    }
    // Table 106's ambiguous (29, 4, id) shape: both fixed layouts are accepted; anything else fails closed.
    let mut key = vec![29u8, 4];
    key.extend_from_slice(&[2u8; 64]);
    for row in [vec![1u8; 4 * 64 + 8], vec![1u8; 3 * 64 + 8]] {
        let mut reader = NativeWireReaderV1::new(row.as_slice(), row.len() as u64);
        let shape = super::wire_kernel_aux::scan_native_aux_row_v1(106, &key, &mut reader).unwrap();
        assert_eq!(reader.remaining(), 0);
        assert!(shape.heap >= row.len() as u64, "the buffered ambiguous row is accounted");
    }
    let odd = vec![1u8; 77];
    let mut reader = NativeWireReaderV1::new(odd.as_slice(), odd.len() as u64);
    assert!(super::wire_kernel_aux::scan_native_aux_row_v1(106, &key, &mut reader).is_err());
}

/// 2026-10-15 renumbering: Codex's material release recovery V2 (134 → 137) and retired compact V4 (135 → 138) no longer share
/// a table with RFC-0016 (132–135) or RFC-0018 (136). Each renumbered table's own key grammar reaches its own codec arm.
#[test]
fn renumbered_codex_aux_tables_never_collide_with_rfc0016_or_rfc0018() {
    use crate::palw_rfc0016_v1::ledger::{
        PALW_RFC0016_QUEUE_INDEX_TABLE_V1, PALW_RFC0016_QUEUE_TABLE_V1, PALW_RFC0016_RIGHTS_TABLE_V1, PALW_RFC0016_TELEMETRY_TABLE_V1,
    };
    let recovery = crate::palw_material_release_recovery_v2::PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2;
    let compact = crate::palw_claim_compact_v4::PAID_CLAIM_COMPACT_V4;
    assert_eq!((recovery, compact), (137, 138));
    let all = [
        crate::palw_beacon_bootstrap_v1::PALW_BEACON_BOOTSTRAP_TABLE_V1,
        crate::palw_model_eligibility_v1::PALW_MODEL_ELIGIBILITY_TABLE_V1,
        PALW_RFC0016_RIGHTS_TABLE_V1,
        PALW_RFC0016_QUEUE_TABLE_V1,
        PALW_RFC0016_QUEUE_INDEX_TABLE_V1,
        PALW_RFC0016_TELEMETRY_TABLE_V1,
        crate::palw_security_certification_v1::PALW_SECURITY_CERTIFICATION_TABLE_V1,
        recovery,
        compact,
        // RFC-0006 consumer v2: the router binding rows.
        crate::palw_router_binding_v2::PALW_ROUTER_BINDING_TABLE_V2,
    ];
    let mut unique = all.to_vec();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), all.len(), "aux table numbers collide: {all:?}");
    // An unallocated key shape inside either renumbered table fails closed as malformed (never as Unsupported).
    for table in [recovery, compact] {
        let mut empty = NativeWireReaderV1::new(&[][..], 0);
        let error = super::wire_kernel_aux::scan_native_aux_row_v1(table, &[0xEE; 7], &mut empty).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData, "table {table}");
    }
    // A 64-byte RFC-0016 queue-index key under 134 still reaches RFC-0016's (Hash64, u64) codec, not the recovery codec.
    let row = borsh::to_vec(&(crate::Hash64::default(), 7u64)).unwrap();
    let mut reader = NativeWireReaderV1::new(row.as_slice(), row.len() as u64);
    super::wire_kernel_aux::scan_native_aux_row_v1(PALW_RFC0016_QUEUE_INDEX_TABLE_V1, &[1u8; 64], &mut reader).unwrap();
    assert_eq!(reader.remaining(), 0);
}

/// RFC-0008 once-only settlement markers (aux 140, dormant): a 64-byte obligation key reaches the marker's derive codec exactly;
/// any other key shape in the table fails closed as malformed; 140 collides with no other aux table. Source only, not executed.
#[test]
fn settlement_once_markers_have_their_own_exact_aux_codec() {
    use crate::palw_settlement_once_v1::*;
    assert_eq!(PALW_SETTLEMENT_ONCE_TABLE_V1, 140);
    for other in [
        crate::palw_material_release_recovery_v2::PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2,
        crate::palw_claim_compact_v4::PAID_CLAIM_COMPACT_V4,
        crate::palw_security_certification_v1::PALW_SECURITY_CERTIFICATION_TABLE_V1,
        crate::palw_model_eligibility_v1::PALW_MODEL_ELIGIBILITY_TABLE_V1,
    ] {
        assert_ne!(other, PALW_SETTLEMENT_ONCE_TABLE_V1);
    }
    let obligation = PalwSettlementObligationV1::of(PalwSettlementLegV1::EscrowFinalReward, [5; 64]);
    let row = PalwSettlementOnceRowV1 {
        version: PALW_SETTLEMENT_ONCE_ROW_VERSION_V1,
        obligation,
        consumer: PalwSettlementConsumerV1::KernelTick,
        amount: 3,
        debit: PalwSettlementDebitV1::Escrow { poster: [6; 64], amount: 3 },
        settled_daa: 9,
    };
    let bytes = borsh::to_vec(&row).unwrap();
    let mut reader = NativeWireReaderV1::new(bytes.as_slice(), bytes.len() as u64);
    super::wire_kernel_aux::scan_native_aux_row_v1(PALW_SETTLEMENT_ONCE_TABLE_V1, &obligation.id_v1(), &mut reader).unwrap();
    assert_eq!(reader.remaining(), 0);
    let mut empty = NativeWireReaderV1::new(&[][..], 0);
    let error = super::wire_kernel_aux::scan_native_aux_row_v1(PALW_SETTLEMENT_ONCE_TABLE_V1, &[0xEE; 65], &mut empty).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

/// RFC-0006 RFC06-8 class registration origins (aux 141, dormant): a 64-byte class key reaches the row's derive codec exactly; any
/// other key shape fails closed as malformed; 141 collides with no other aux table. Source only, not executed.
#[test]
fn class_registration_origins_have_their_own_exact_aux_codec() {
    use crate::palw_class_registration_origin_v1::*;
    assert_eq!(PALW_CLASS_REGISTRATION_ORIGIN_TABLE_V1, 141);
    for other in [
        crate::palw_settlement_once_v1::PALW_SETTLEMENT_ONCE_TABLE_V1,
        crate::palw_router_binding_v2::PALW_ROUTER_BINDING_TABLE_V2,
        crate::palw_material_release_recovery_v2::PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2,
        crate::palw_claim_compact_v4::PAID_CLAIM_COMPACT_V4,
        crate::palw_security_certification_v1::PALW_SECURITY_CERTIFICATION_TABLE_V1,
    ] {
        assert_ne!(other, PALW_CLASS_REGISTRATION_ORIGIN_TABLE_V1);
    }
    let class_id = crate::Hash64::from_bytes([4; 64]);
    let row = PalwClassRegistrationOriginRowV1 {
        version: PALW_CLASS_REGISTRATION_ORIGIN_ROW_VERSION_V1,
        class_id,
        block: crate::Hash64::from_bytes([5; 64]),
        daa_score: 9,
        door: PalwClassRegistrationDoorV1::Legacy,
        admission_digest: Some(crate::Hash64::from_bytes([6; 64])),
    };
    let bytes = borsh::to_vec(&row).unwrap();
    let mut reader = NativeWireReaderV1::new(bytes.as_slice(), bytes.len() as u64);
    super::wire_kernel_aux::scan_native_aux_row_v1(
        PALW_CLASS_REGISTRATION_ORIGIN_TABLE_V1,
        &palw_class_registration_origin_key_v1(&class_id),
        &mut reader,
    )
    .unwrap();
    assert_eq!(reader.remaining(), 0);
    let mut empty = NativeWireReaderV1::new(&[][..], 0);
    let error = super::wire_kernel_aux::scan_native_aux_row_v1(PALW_CLASS_REGISTRATION_ORIGIN_TABLE_V1, &[0xEE; 65], &mut empty).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

/// RFC-0007 §4.1 T6 paid-claim trap rows (aux 142, dormant): a 65-byte `0x01 ‖ claim` key reaches the conviction row's codec, a
/// 137-byte `0x02 ‖ claim ‖ verifier ‖ le64(slot)` key the held check fee's; any other shape fails closed as malformed; 142 collides
/// with no other aux table. Source only, not executed.
#[test]
fn paid_claim_trap_rows_have_their_own_exact_aux_codec() {
    use crate::palw_paid_claim_trap_v1::*;
    assert_eq!(PALW_PAID_CLAIM_TRAP_TABLE_V1, 142);
    for other in [
        crate::palw_settlement_once_v1::PALW_SETTLEMENT_ONCE_TABLE_V1,
        crate::palw_class_registration_origin_v1::PALW_CLASS_REGISTRATION_ORIGIN_TABLE_V1,
        crate::palw_router_binding_v2::PALW_ROUTER_BINDING_TABLE_V2,
        crate::palw_material_release_recovery_v2::PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2,
        crate::palw_claim_compact_v4::PAID_CLAIM_COMPACT_V4,
        crate::palw_security_certification_v1::PALW_SECURITY_CERTIFICATION_TABLE_V1,
    ] {
        assert_ne!(other, PALW_PAID_CLAIM_TRAP_TABLE_V1);
    }
    let conviction = PalwPaidClaimConvictionRowV1 {
        version: PALW_PAID_CLAIM_TRAP_ROW_VERSION_V1,
        base_claim: [1; 64],
        enrollment: [2; 64],
        accuser: [3; 64],
        convicted_daa: 9,
        forget_after_daa: 10,
    };
    let bytes = borsh::to_vec(&conviction).unwrap();
    let mut reader = NativeWireReaderV1::new(bytes.as_slice(), bytes.len() as u64);
    super::wire_kernel_aux::scan_native_aux_row_v1(PALW_PAID_CLAIM_TRAP_TABLE_V1, &palw_paid_trap_conviction_key_v1(&[1; 64]), &mut reader)
        .unwrap();
    assert_eq!(reader.remaining(), 0);
    let held = PalwHeldCheckFeeRowV1 {
        version: PALW_PAID_CLAIM_TRAP_ROW_VERSION_V1,
        claim: [1; 64],
        verifier: [4; 64],
        slot: 0,
        poster: [5; 64],
        fee: 7,
        release_after_daa: 11,
    };
    let bytes = borsh::to_vec(&held).unwrap();
    let mut reader = NativeWireReaderV1::new(bytes.as_slice(), bytes.len() as u64);
    super::wire_kernel_aux::scan_native_aux_row_v1(
        PALW_PAID_CLAIM_TRAP_TABLE_V1,
        &palw_paid_trap_held_fee_key_v1(&[1; 64], &[4; 64], 0),
        &mut reader,
    )
    .unwrap();
    assert_eq!(reader.remaining(), 0);
    for bad_key in [vec![0xEE; 65], vec![PALW_PAID_CLAIM_TRAP_CONVICTION_PREFIX_V1; 137], vec![0xEE; 64]] {
        let mut empty = NativeWireReaderV1::new(&[][..], 0);
        let error = super::wire_kernel_aux::scan_native_aux_row_v1(PALW_PAID_CLAIM_TRAP_TABLE_V1, &bad_key, &mut empty).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }
}
