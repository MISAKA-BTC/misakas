//! **RFC-0007 Parts II and IV, the pure half** (`kaspa_consensus_core::palw_mesh_v1`): the witness profile read off a program, the v2
//! trace manifest, the window term, the audit draw, the trap commitment and its slot lottery.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_mesh_v1`

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_mesh_v1::*;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir::{DType, Ref, TensorType, TirProgramV1};

fn bond(i: u8) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_bytes([i; 64]), u32::from(i)))
}

fn h(i: u8) -> Hash64 {
    Hash64::from_bytes([i; 64])
}

/// A one-layer program: the layer holds one weight product `W[k, k] · col[k, 1]`, the head one `H[8, k] · col[k, 1]`. A weight
/// product is served when its contraction `k` is at least 64 and recomputed below it.
fn program(k: u32) -> TirProgramV1 {
    let mut pb = ProgramBuilder::new(8, HISTORY_BOUND_V1_SMALL);
    let embed = pb.param("embed", DType::I8, &[8, k], false);
    let w = pb.param("w", DType::I8, &[k, k], true);
    let head = pb.param("head", DType::I8, &[8, k], false);
    let carry = vec![TensorType::fixed(DType::I32, &[k])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let x = b.gather(embed, Ref::Input(0), 0, 0);
        let x = b.cast(x, DType::I32);
        b.finish(&[x])
    };
    let layer = {
        let mut b = pb.block("layer", carry.clone());
        let x = Ref::CarryIn(0);
        let col = b.reshape_fixed(x, &[k, 1]);
        let y = b.matmul(w, col, DType::I64);
        let y = b.reshape_fixed(y, &[k]);
        let y = b.clamp(y, -1000, 1000, DType::I32);
        let z = b.add(x, y, DType::I32);
        let z = b.clamp(z, -1000, 1000, DType::I32);
        b.finish(&[z])
    };
    let (post, logits) = {
        let mut b = pb.block("post", carry);
        let col = b.reshape_fixed(Ref::CarryIn(0), &[k, 1]);
        let l = b.matmul(head, col, DType::I64);
        let l = b.clamp(l, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let l = b.reshape_fixed(l, &[8]);
        let l = b.commit(l);
        let Ref::Node(i) = l else { unreachable!() };
        (b.finish(&[]), i)
    };
    let mut p = pb.finish(pre, vec![layer], post, logits);
    p.logits_scheme_id.copy_from_slice(kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1().as_byte_slice());
    p
}

#[test]
fn the_canonical_serving_set_pins_the_witness_profile() {
    // k = 64: the layer's product (64 elements) and the head's (8) are served: 72 elements a position, 8 bytes each.
    let p = program(64);
    let profile = palw_witness_profile_v1(&p, 100).expect("a class that serves a witness has a profile");
    assert_eq!(profile.elements_per_position, 72);
    assert_eq!(profile.max_context, 100);
    assert_eq!(profile.bytes, 72 * 100 * 8);
    assert_eq!(profile.chunks, 1, "57,600 bytes fit one chunk");
    // A longer context: more bytes, more chunks (1 MiB each), capped.
    let big = palw_witness_profile_v1(&p, 1_000_000).expect("a profile");
    assert_eq!(big.bytes, 72 * 1_000_000 * 8);
    assert_eq!(big.chunks as u64, big.bytes.div_ceil(PALW_WITNESS_CHUNK_BYTES_V1));
    let capped = palw_witness_profile_v1(&p, u32::MAX).expect("a profile");
    assert!(capped.chunks <= PALW_WITNESS_MAX_CHUNKS_V1, "never more than the cap");
    // k = 16: every product has a contraction below 64, so the seat recomputes them all and nothing is served: no profile, the v1 manifest.
    assert!(palw_witness_profile_v1(&program(16), 100).is_none());
    assert!(palw_witness_profile_v1(&p, 0).is_none(), "no context, no witness");
    // The profile is a function of the program alone: the same program, the same profile.
    assert_eq!(palw_witness_profile_v1(&program(64), 100), Some(profile));
}

#[test]
fn the_v2_manifest_root_is_its_own_domain_and_binds_the_count() {
    let trace = h(1);
    let v1 = kaspa_consensus_core::palw_attempt_v2::attempt_trace_manifest_root_v1(trace, 1);
    let v2 = palw_attempt_trace_manifest_root_v2(trace, 1);
    assert_ne!(v1, v2, "a v1 root is never read as a v2 one");
    assert_ne!(palw_attempt_trace_manifest_root_v2(trace, 2), palw_attempt_trace_manifest_root_v2(trace, 3), "the count is bound");
    assert_ne!(palw_attempt_trace_manifest_root_v2(trace, 2), palw_attempt_trace_manifest_root_v2(h(2), 2), "the trace root is bound");
    // The stateless pin: a count of 1 under the v1 root, 2.. under the v2 one, nothing else.
    assert!(palw_witness_manifest_shape_ok_v1(trace, 1, v1).is_ok());
    assert!(palw_witness_manifest_shape_ok_v1(trace, 5, palw_attempt_trace_manifest_root_v2(trace, 5)).is_ok());
    assert!(matches!(palw_witness_manifest_shape_ok_v1(trace, 0, v1), Err(WitnessPinRefusalV1::CountOutOfRange { count: 0 })));
    assert!(matches!(
        palw_witness_manifest_shape_ok_v1(trace, 2 + PALW_WITNESS_MAX_CHUNKS_V1, v2),
        Err(WitnessPinRefusalV1::CountOutOfRange { .. })
    ));
    assert!(matches!(palw_witness_manifest_shape_ok_v1(trace, 5, v1), Err(WitnessPinRefusalV1::ManifestNotDerived { .. })));
    assert!(matches!(palw_witness_manifest_shape_ok_v1(trace, 1, v2), Err(WitnessPinRefusalV1::ManifestNotDerived { .. })));
}

#[test]
fn an_unavailable_names_a_witness_chunk_and_the_trace_is_chunk_zero() {
    assert!(!palw_unavailable_names_witness_chunk_v1(0, 5), "chunk 0 is the trace");
    assert!(palw_unavailable_names_witness_chunk_v1(1, 5) && palw_unavailable_names_witness_chunk_v1(4, 5));
    assert!(!palw_unavailable_names_witness_chunk_v1(5, 5), "past the count is no chunk");
    assert!(!palw_unavailable_names_witness_chunk_v1(1, 1), "a class with no witness has none to name");
}

#[test]
fn the_canonical_chunk_count_and_the_window_term() {
    assert_eq!(palw_witness_canonical_chunk_count_v1(None), 1);
    let profile = PalwWitnessProfileRowV1 { elements_per_position: 1_000, max_context: 64, bytes: 512_000, chunks: 3 };
    assert_eq!(palw_witness_canonical_chunk_count_v1(Some(&profile)), 4);
    assert_eq!(palw_witness_ccu_v1(0), 0);
    assert_eq!(palw_witness_ccu_v1(1_000_000), 320_000_000, "320 MAC-eq a byte: a byte at 100 Mbit/s against 4 MMAC-eq/s");
    assert_eq!(palw_witness_ccu_v1(u64::MAX), u128::from(u64::MAX) * 320, "no wrap");
}

fn candidates(n: u8, weight: impl Fn(u8) -> u64) -> Vec<PalwAuditCandidateV1> {
    (1..=n).map(|i| PalwAuditCandidateV1 { bond: bond(i), operator_id: h(100 + i), weight_msk: weight(i) }).collect()
}

#[test]
fn the_audit_draw_is_deterministic_distinct_by_operator_and_post_commit() {
    let cands = candidates(20, |_| 130_000);
    let seed = palw_mesh_audit_seed_v1(&h(1), &h(2), &h(3), 100);
    let a = palw_mesh_audit_draw_v1(&seed, &cands, PALW_AUDITS_PER_CLAIM_V1);
    let b = palw_mesh_audit_draw_v1(&seed, &cands, PALW_AUDITS_PER_CLAIM_V1);
    assert_eq!(a, b, "deterministic");
    assert_eq!(a.len(), PALW_AUDITS_PER_CLAIM_V1);
    assert_ne!(a[0].0, a[1].0, "two distinct auditors");
    // The draw is a function of the claim, its carrying block, its execution root and the DAA: change any and it moves.
    let moved = [
        palw_mesh_audit_seed_v1(&h(9), &h(2), &h(3), 100),
        palw_mesh_audit_seed_v1(&h(1), &h(9), &h(3), 100),
        palw_mesh_audit_seed_v1(&h(1), &h(2), &h(9), 100),
        palw_mesh_audit_seed_v1(&h(1), &h(2), &h(3), 101),
    ];
    assert!(moved.iter().all(|s| *s != seed));
    let draws: Vec<_> = moved.iter().map(|s| palw_mesh_audit_draw_v1(s, &cands, 2)).collect();
    assert!(draws.iter().any(|d| *d != a), "a different seed deals different auditors somewhere");
    // One seat per operator: two bonds of one operator never both sit.
    let mut twins = candidates(6, |_| 100);
    for c in &mut twins {
        c.operator_id = h(7);
    }
    assert_eq!(palw_mesh_audit_draw_v1(&seed, &twins, 3).len(), 1, "one operator, one seat");
    // Fewer candidates than the count: all of them; none: none.
    assert_eq!(palw_mesh_audit_draw_v1(&seed, &candidates(1, |_| 1), 2).len(), 1);
    assert!(palw_mesh_audit_draw_v1(&seed, &[], 2).is_empty());
}

#[test]
fn the_audit_draw_follows_stake() {
    // One heavy bond against nineteen light ones: over many seeds it sits far more often than 1/20.
    let cands = candidates(20, |i| if i == 1 { 1_000_000 } else { 10_000 });
    let mut heavy = 0;
    let rounds = 400u32;
    for round in 0..rounds {
        let seed = palw_mesh_audit_seed_v1(&h(1), &h(2), &h(3), u64::from(round));
        if palw_mesh_audit_draw_v1(&seed, &cands, 2).iter().any(|(b, _)| *b == bond(1)) {
            heavy += 1;
        }
    }
    assert!(heavy > rounds * 8 / 10, "the heavy bond sat in {heavy} of {rounds} draws");
    // Equal stake: a given bond sits in about 2 of 20 draws.
    let equal = candidates(20, |_| 50_000);
    let mut sat = 0;
    for round in 0..2_000u32 {
        let seed = palw_mesh_audit_seed_v1(&h(4), &h(5), &h(6), u64::from(round));
        if palw_mesh_audit_draw_v1(&seed, &equal, 2).iter().any(|(b, _)| *b == bond(3)) {
            sat += 1;
        }
    }
    assert!((120..=280).contains(&sat), "an equal-stake bond sat {sat} times of 2,000 (expect ≈ 200)");
}

#[test]
fn pay_penalty_and_bounty_follow_the_decision() {
    // Question 7, settled 2026-10-03: pay 4 ‰ of the escrowed reward per audit, penalty 10× the pay, bounty 5× the pay.
    let pay = palw_mesh_audit_pay_v1(5_000_000_000);
    assert_eq!(pay, 20_000_000);
    assert_eq!(palw_mesh_trap_penalty_v1(pay), 10 * 20_000_000);
    assert_eq!(palw_mesh_trap_bounty_v1(pay), 5 * 20_000_000);
    assert_eq!(palw_mesh_audit_pay_v1(0), 0);
    assert_eq!(palw_mesh_audit_pay_v1(u64::MAX), ((u128::from(u64::MAX) * 4) / 1_000) as u64, "no wrap");
    assert_eq!(PALW_AUDITS_PER_CLAIM_V1, 2);
    assert_eq!(PALW_TRAP_RATE_BP_V1, 100, "1 %");
    assert_eq!(PALW_TRAP_PENALTY_MULTIPLE_V1, 10);
    assert_eq!(PALW_CAPPED_WEIGHT_CAP_PERMILLE_V1, 10, "w_cap is 1 %");
}

#[test]
fn a_trap_commitment_binds_its_claim_its_tile_its_count_and_its_salt() {
    let salt = [7u8; 32];
    let c = palw_trap_commitment_v1(&h(1), 3, 8, &salt);
    assert_ne!(c, palw_trap_commitment_v1(&h(2), 3, 8, &salt), "claim");
    assert_ne!(c, palw_trap_commitment_v1(&h(1), 4, 8, &salt), "tile");
    assert_ne!(c, palw_trap_commitment_v1(&h(1), 3, 9, &salt), "count");
    assert_ne!(c, palw_trap_commitment_v1(&h(1), 3, 8, &[8u8; 32]), "salt");
    let reveal = PalwTrapRevealedV1 { setter_bond: bond(1), claim: h(1), fault_leaf: 3, tiles: 8, salt, signature: vec![] };
    assert_eq!(reveal.commitment(), c, "a reveal opens exactly its commitment");
    // The messages are bound to the network and the setter, and are not each other.
    let m1 = palw_trap_committed_message_v1(h(0xD0), &bond(1), &c);
    assert_ne!(m1, palw_trap_committed_message_v1(h(0xD1), &bond(1), &c));
    assert_ne!(m1, palw_trap_committed_message_v1(h(0xD0), &bond(2), &c));
    let m2 = palw_trap_revealed_message_v1(h(0xD0), &bond(1), &h(1), 3, 8, &salt);
    assert_ne!(m1, m2, "a commitment's signature is not a reveal's");
    assert_ne!(m2, palw_trap_revealed_message_v1(h(0xD0), &bond(1), &h(1), 4, 8, &salt), "the tile is signed");
    // Hitting: the auditor audits tile `ticket % tiles`.
    assert!(palw_trap_ticket_hits_v1(11, 3, 8));
    assert!(!palw_trap_ticket_hits_v1(12, 3, 8));
    assert!(!palw_trap_ticket_hits_v1(12, 0, 0), "no tiles, no hit");
}

#[test]
fn the_trap_slot_lottery_draws_about_one_percent() {
    let drawn = (0..20_000u32)
        .filter(|i| {
            let b = PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_bytes([(*i % 251) as u8; 64]), *i));
            palw_trap_slot_drawn_v1(&b, u64::from(*i / 7) * PALW_TRAP_SLOT_DAA_V1)
        })
        .count();
    assert!((120..=300).contains(&drawn), "{drawn} of 20,000 slots drawn (expect ≈ 200)");
    // One draw per slot: every DAA of a slot gives one answer.
    let b = bond(5);
    for slot in 0..50u64 {
        let first = palw_trap_slot_drawn_v1(&b, slot * PALW_TRAP_SLOT_DAA_V1);
        assert_eq!(first, palw_trap_slot_drawn_v1(&b, slot * PALW_TRAP_SLOT_DAA_V1 + PALW_TRAP_SLOT_DAA_V1 - 1));
    }
}

#[test]
fn the_mesh_domains_are_unique() {
    let mut all: Vec<&[u8]> = PALW_MESH_V1_ALL_DOMAINS.to_vec();
    all.extend(kaspa_consensus_core::palw_vertex_v1::PALW_VERTEX_V1_ALL_DOMAINS);
    all.extend(kaspa_consensus_core::palw_receipt::PALW_RECEIPT_ALL_DOMAINS);
    all.push(PALW_MESH_TRAP_COMMITTED_MLDSA87_CONTEXT_V1);
    all.push(PALW_MESH_TRAP_REVEALED_MLDSA87_CONTEXT_V1);
    for (i, a) in all.iter().enumerate() {
        for b in all.iter().skip(i + 1) {
            assert_ne!(a, b, "domain collision: {:?}", String::from_utf8_lossy(a));
        }
    }
}

#[test]
fn the_weight_cap_arithmetic() {
    assert!(palw_capped_weight_admits_v1(0, 10));
    assert!(!palw_capped_weight_admits_v1(0, 11));
    assert!(palw_capped_weight_admits_v1(6, 4));
    assert!(!palw_capped_weight_admits_v1(6, 5));
    assert!(!palw_capped_weight_admits_v1(u32::MAX, 1), "no wrap");
}

/// **The canonical serving set on the corpus**: every program of the consensus vectors reads to a profile (or to none) without panicking, the
/// dense corpus programs serve something, and a profile is a function of the program alone.
#[test]
fn the_corpus_programs_read_to_witness_profiles() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v1/programs");
    let mut with_profile = Vec::new();
    let mut seen = 0;
    for entry in std::fs::read_dir(&dir).expect("the corpus") {
        let path = entry.expect("an entry").path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let hex = v["program_borsh_hex"].as_str().expect("the program");
        let bytes: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
        let program = TirProgramV1::decode_canonical(&bytes).expect("a canonical program");
        seen += 1;
        for context in [1u32, 64, 4_096] {
            let a = palw_witness_profile_v1(&program, context);
            assert_eq!(a, palw_witness_profile_v1(&program, context), "a function of the program alone");
            if let Some(profile) = a {
                assert_eq!(profile.bytes, profile.elements_per_position * u64::from(context) * 8);
                assert!(profile.chunks >= 1 && profile.chunks <= PALW_WITNESS_MAX_CHUNKS_V1);
                if context == 64 {
                    with_profile.push((path.file_name().unwrap().to_string_lossy().to_string(), profile));
                }
            }
        }
    }
    assert!(seen >= 5, "the corpus is there ({seen} programs)");
    println!("witness profiles at 64 positions: {with_profile:#?}");
}
