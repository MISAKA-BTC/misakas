//! The V2-lineage PoW arms (ADR-0042 Decision 3a Unit A, ADR-0044 Decision 6 Unit B), and the
//! properties that make them safe to have landed.
//!
//! This file replaces the tripwire that recorded their ABSENCE. That tripwire existed so the
//! arms could not appear without someone deliberately editing the test that said they were
//! missing — which is exactly what happened here, in the commit that added them.
//!
//! What is pinned now:
//!
//! 1. a header of either lane carrying NO decodable carriage is a failed PoW, never a panic —
//!    the remote-crash P0's rule, extended to the two new ids;
//! 2. the attempt lane (6) prices its digest against the target, as every hash lane does;
//! 3. the receipt lane (7) does NOT — its digest is identity binding, its lottery is admission —
//!    and, for the same reason, it buys no block level.

use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::palw_attempt_v2::{PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, challenge_v2};
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_V3_VERSION, PalwReceiptSpendEnvelopeV3, PalwReceiptSpendUnsignedV3, spend_challenge_v3,
};
use kaspa_consensus_core::pow_layer0::{
    POW_ALGO_ID_KHEAVYHASH, POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_RECEIPT_V3, PowLayer0Error,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;

const NETWORK: &[u8] = b"fp-probe";

fn header_with(algo_id: u8, carriage: Vec<u8>) -> Header {
    let mut header = Header::from_precomputed_hash(Hash64::from_u64_word(0xB10C), vec![Hash64::from_u64_word(0xBEEF)]);
    header.pow_algo_id = algo_id;
    header.palw_commitment = carriage;
    header.bits = 0x207fffff;
    header
}

/// Each lane names its own absence. The attempt lane's error predates the receipt lane and is
/// referenced by name elsewhere (`palw_v2_commitment_mutation_invalidates_pow`), so it kept its
/// specific name rather than being folded into the generic one.
fn expected_missing_carriage_error(algo_id: u8) -> PowLayer0Error {
    if algo_id == POW_ALGO_ID_PALW_COMMITTED_V2 {
        PowLayer0Error::PalwV2AttemptMissing
    } else {
        PowLayer0Error::PalwCarriageMissing(algo_id)
    }
}

fn bond() -> TransactionOutpoint {
    TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0)
}

/// An attempt envelope bound to `header`'s own position — what a miner would carry.
fn attempt_carriage(header: &Header) -> Vec<u8> {
    let network_domain = Hash64::from_u64_word(0x4E);
    let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
    let class = Hash64::from_u64_word(0xC1);
    let attempt = PalwAttemptUnsignedV2 {
        version: PALW_ATTEMPT_V2_VERSION,
        network_domain,
        challenge: challenge_v2(network_domain, pre_pow, header.timestamp, header.nonce, class, &bond()),
        class_id: class,
        executor_bond: bond(),
        executor_pubkey: vec![7u8; 32],
        operator_id: Hash64::from_u64_word(0xE0),
        artifact_root: Hash64::from_u64_word(0xA7),
        trace_root: Hash64::from_u64_word(0x7A),
        output_root: Hash64::from_u64_word(0x00),
        execution_root: Hash64::from_u64_word(0x4E),
        pwu: 4_242,
        trace_manifest_root: Hash64::from_u64_word(0xD0),
        trace_chunk_count: 8,
        trace_retention_daa: 999_999,
    };
    PalwAttemptEnvelopeV2 { attempt, signature: vec![0x5A; kaspa_consensus_core::dns_finality::STAKE_ATTESTATION_SIG_LEN] }
        .encode_wire()
}

fn spend_carriage(header: &Header, quantum_index: u32) -> Vec<u8> {
    let network_domain = Hash64::from_u64_word(0x4E);
    let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
    let claim_id = Hash64::from_u64_word(0xFC);
    let spend = PalwReceiptSpendUnsignedV3 {
        version: PALW_FP_V3_VERSION,
        network_domain,
        challenge: spend_challenge_v3(network_domain, pre_pow, header.timestamp, header.nonce, claim_id, quantum_index, &bond()),
        claim_id,
        quantum_index,
        beacon_block: Hash64::from_u64_word(0xBEAC),
        producer_bond: bond(),
        producer_pubkey: vec![7u8; 32],
    };
    PalwReceiptSpendEnvelopeV3 { spend, signature: vec![0x5A; kaspa_consensus_core::dns_finality::STAKE_ATTESTATION_SIG_LEN] }.encode()
}

/// **The rule the removed tripwire really protected**: a peer-controlled header must never panic
/// the finalizer. A V2-lineage id with no carriage (or a foreign lane's carriage) is a returned
/// error, and the block-level entry maps it to a failed PoW at level 0.
#[test]
fn a_v2_lineage_header_without_its_carriage_is_a_failed_pow_not_a_panic() {
    let attempt_header = header_with(POW_ALGO_ID_PALW_COMMITTED_V2, Vec::new());
    let spend_header = header_with(POW_ALGO_ID_PALW_RECEIPT_V3, Vec::new());
    // …and each lane's own carriage is refused by the OTHER lane (disjoint magics).
    let crossed_attempt = header_with(POW_ALGO_ID_PALW_COMMITTED_V2, spend_carriage(&spend_header, 0));
    let crossed_spend = header_with(POW_ALGO_ID_PALW_RECEIPT_V3, attempt_carriage(&attempt_header));

    for header in [&attempt_header, &spend_header, &crossed_attempt, &crossed_spend] {
        let state = kaspa_pow::StateLayer0::new(header, NETWORK);
        assert_eq!(
            state.check_pow_layer0(header.nonce).unwrap_err(),
            expected_missing_carriage_error(header.pow_algo_id),
            "algo {} with no usable carriage must answer an error, not a tag",
            header.pow_algo_id
        );
        let (level, passed) = kaspa_pow::calc_block_level_check_pow_layer0(header, NETWORK, 255);
        assert!(!passed && level == 0, "an unverifiable header neither passes nor carries a level");
    }

    // Unknown ids keep the same shape — nothing about the new arms widened what is accepted.
    for unknown in [0u8, 8, 200] {
        let header = header_with(unknown, Vec::new());
        let (level, passed) = kaspa_pow::calc_block_level_check_pow_layer0(&header, NETWORK, 255);
        assert!(!passed && level == 0);
    }
}

/// **Unit A**: the attempt lane's tag is `Expand(commitment_root)`, its digest is priced against
/// the target like any hash lane, and mutating a priced field of the carriage moves the digest —
/// the P0-1 property, now measured through the LIVE path rather than a unit helper.
#[test]
fn the_attempt_lane_prices_its_digest_and_binds_every_priced_field() {
    let header = header_with(POW_ALGO_ID_PALW_COMMITTED_V2, Vec::new());
    let carriage = attempt_carriage(&header);
    let solved = header_with(POW_ALGO_ID_PALW_COMMITTED_V2, carriage.clone());

    let state = kaspa_pow::StateLayer0::new(&solved, NETWORK);
    let (_, digest) = state.check_pow_layer0(solved.nonce).expect("a carried attempt tags");

    // The lane IS priced, asserted the only deterministic way: at an unmeetable target it fails.
    // (A first draft asserted "passes at the devnet target" — which, once the 256-bit target is
    // lifted into the 512-bit digest space, is a coin flip on a fixed fixture. It passed here and
    // failed in the sibling test below, which is how the flaw announced itself.)
    // The unmeetable-target header is BUILT at that target, not edited into it: `bits` is in the
    // pre-PoW preimage, and the carried challenge binds the header position (audit P0-1), so
    // re-pricing a solved attempt by swapping its bits is exactly what the binding refuses. A
    // miner facing a harder target re-derives its challenge; so does this fixture.
    let mut impossible = header_with(POW_ALGO_ID_PALW_COMMITTED_V2, Vec::new());
    impossible.bits = 0x03000001;
    impossible.palw_commitment = attempt_carriage(&impossible);
    let (impossible_passed, _) = kaspa_pow::StateLayer0::new(&impossible, NETWORK).check_pow_layer0(impossible.nonce).expect("tags");
    assert!(!impossible_passed, "an attempt digest above its target must fail — the comparison happened");

    // Every field the commitment root prices moves the digest, through the live path. These are
    // the fields OUTSIDE the challenge equation: the attempt still binds its position, so it
    // still tags, and what changes is the price.
    let mut envelope = PalwAttemptEnvelopeV2::decode_wire(&carriage).unwrap();
    let priced: Vec<(&str, fn(&mut PalwAttemptUnsignedV2))> = vec![
        ("trace_root", |a| a.trace_root = Hash64::from_u64_word(0xDEAD)),
        ("output_root", |a| a.output_root = Hash64::from_u64_word(0xBEEF)),
        ("execution_root", |a| a.execution_root = Hash64::from_u64_word(0xFEED)),
        ("pwu", |a| a.pwu += 1),
    ];
    for (field, mutate) in priced {
        let mut mutated = envelope.clone();
        mutate(&mut mutated.attempt);
        let header = header_with(POW_ALGO_ID_PALW_COMMITTED_V2, mutated.encode_wire());
        let state = kaspa_pow::StateLayer0::new(&header, NETWORK);
        let (_, mutated_digest) = state.check_pow_layer0(header.nonce).expect("still tags");
        assert_ne!(mutated_digest, digest, "mutating {field} left the live PoW digest unchanged");
    }

    // The fields INSIDE the challenge equation get the stronger answer: the finalizer recomputes
    // the challenge from the header position and REFUSES, rather than pricing a re-mounted
    // attempt at a new digest. That is audit P0-1 at its strongest — a solved attempt cannot be
    // re-pointed at another class or another bond at any price.
    let bound: Vec<(&str, fn(&mut PalwAttemptUnsignedV2))> = vec![
        ("class_id", |a| a.class_id = Hash64::from_u64_word(0xC2)),
        ("executor_bond", |a| a.executor_bond.index += 1),
        ("challenge", |a| a.challenge = Hash64::from_u64_word(0x1234)),
    ];
    for (field, mutate) in bound {
        let mut mutated = envelope.clone();
        mutate(&mut mutated.attempt);
        let header = header_with(POW_ALGO_ID_PALW_COMMITTED_V2, mutated.encode_wire());
        let state = kaspa_pow::StateLayer0::new(&header, NETWORK);
        assert_eq!(
            state.check_pow_layer0(header.nonce).unwrap_err(),
            PowLayer0Error::PalwV2ChallengeMismatch,
            "mutating {field} must break the position binding, not just move the price"
        );
    }

    // The signature is NOT priced — identity is the unsigned attempt (Decision 3c), so a second
    // valid signature is not a second PoW.
    envelope.signature = vec![0xA5; envelope.signature.len()];
    let resigned = header_with(POW_ALGO_ID_PALW_COMMITTED_V2, envelope.encode_wire());
    let state = kaspa_pow::StateLayer0::new(&resigned, NETWORK);
    let (_, resigned_digest) = state.check_pow_layer0(resigned.nonce).expect("still tags");
    assert_eq!(resigned_digest, digest, "the signature must not reach the PoW digest");
}

/// **Unit B, the load-bearing difference**: the receipt lane's digest is identity binding. It
/// passes whatever the target says (its lottery is admission), it buys NO block level, and it
/// still moves with every field of the spend — so one signature cannot mint two identities.
#[test]
fn the_receipt_lane_binds_identity_without_pricing_or_buying_level() {
    let base = header_with(POW_ALGO_ID_PALW_RECEIPT_V3, Vec::new());
    let carriage = spend_carriage(&base, 0);
    let header = header_with(POW_ALGO_ID_PALW_RECEIPT_V3, carriage.clone());

    // Even at the HARDEST possible target, the receipt lane passes: the bits comparison is not
    // its work. (An attempt header at this target would fail — that contrast is the test.)
    // Built at the target, not edited into it: both lanes' challenges bind the header position,
    // and `bits` is in the pre-PoW preimage (audit P0-1).
    let mut impossible = header_with(POW_ALGO_ID_PALW_RECEIPT_V3, Vec::new());
    impossible.bits = 0x03000001;
    impossible.palw_commitment = spend_carriage(&impossible, 0);
    let state = kaspa_pow::StateLayer0::new(&impossible, NETWORK);
    let (passed, _) = state.check_pow_layer0(impossible.nonce).expect("tags");
    assert!(passed, "the receipt lane's digest is identity binding — bits do not price it");

    let mut attempt_at_same_bits = header_with(POW_ALGO_ID_PALW_COMMITTED_V2, Vec::new());
    attempt_at_same_bits.bits = 0x03000001;
    attempt_at_same_bits.palw_commitment = attempt_carriage(&attempt_at_same_bits);
    let attempt_state = kaspa_pow::StateLayer0::new(&attempt_at_same_bits, NETWORK);
    let (attempt_passed, _) = attempt_state.check_pow_layer0(attempt_at_same_bits.nonce).expect("tags");
    assert!(!attempt_passed, "the attempt lane at the same impossible target must fail — the lanes differ");

    // A free digest must not buy hierarchy position: the receipt lane sits at the base level for
    // EVERY nonce, while the attempt lane's level still varies with its digest (so the arm did
    // not flatten the hierarchy for everyone).
    //
    // **This assertion only became true of the stored level in this commit.** It goes through
    // `calc_block_level_check_pow_layer0`, and until now the ordinary block path did NOT — the
    // header processor kept its own copy of the split, without this clamp, and that copy is what
    // wrote the level into the headers store. The pipeline delegates here now, so there is one
    // implementation and this test pins it. If a future change reintroduces a second copy, this
    // test goes back to passing for a reason other than the one its name states.
    let mut attempt_saw_a_level = false;
    for nonce in 0..24u64 {
        let mut receipt = header.clone();
        receipt.nonce = nonce;
        receipt.palw_commitment = spend_carriage(&receipt, 0);
        let (receipt_level, receipt_passed) = kaspa_pow::calc_block_level_check_pow_layer0(&receipt, NETWORK, 255);
        assert!(receipt_passed && receipt_level == 0, "a receipt block is level 0 at nonce {nonce}, always");

        let mut attempt = header_with(POW_ALGO_ID_PALW_COMMITTED_V2, Vec::new());
        attempt.nonce = nonce;
        attempt.palw_commitment = attempt_carriage(&attempt);
        let (attempt_level, _) = kaspa_pow::calc_block_level_check_pow_layer0(&attempt, NETWORK, 255);
        attempt_saw_a_level |= attempt_level > 0;
    }
    assert!(attempt_saw_a_level, "the attempt lane still derives levels from its digest");

    // …and identity is still total: every field of the spend moves the digest, so one signature
    // is one block, not a family of them.
    let envelope = PalwReceiptSpendEnvelopeV3::decode(&carriage).unwrap();
    let (_, digest) = kaspa_pow::StateLayer0::new(&header, NETWORK).check_pow_layer0(header.nonce).unwrap();
    let mutations: Vec<(&str, fn(&mut PalwReceiptSpendUnsignedV3))> = vec![
        ("challenge", |s| s.challenge = Hash64::from_u64_word(0x1234)),
        ("claim_id", |s| s.claim_id = Hash64::from_u64_word(0xF00)),
        ("quantum_index", |s| s.quantum_index += 1),
        ("beacon_block", |s| s.beacon_block = Hash64::from_u64_word(0xBEAD)),
        ("producer_bond", |s| s.producer_bond.index += 1),
    ];
    for (field, mutate) in mutations {
        let mut mutated = envelope.clone();
        mutate(&mut mutated.spend);
        let mutated_header = header_with(POW_ALGO_ID_PALW_RECEIPT_V3, mutated.encode());
        let (_, mutated_digest) =
            kaspa_pow::StateLayer0::new(&mutated_header, NETWORK).check_pow_layer0(mutated_header.nonce).unwrap();
        assert_ne!(mutated_digest, digest, "mutating {field} left the receipt digest unchanged");
    }
}

/// The hash lanes are untouched by either arm: they still compute, and they still earn levels
/// from their digests (checked across a sweep, because any single digest's level is a coin flip).
#[test]
fn the_hash_lanes_are_unchanged() {
    let mut saw_a_level = false;
    for nonce in 0..24u64 {
        let mut header = header_with(POW_ALGO_ID_KHEAVYHASH, Vec::new());
        header.nonce = nonce;
        let state = kaspa_pow::StateLayer0::new(&header, NETWORK);
        assert!(state.check_pow_layer0(header.nonce).is_ok(), "the hash lane still computes");
        let (level, _) = kaspa_pow::calc_block_level_check_pow_layer0(&header, NETWORK, 255);
        saw_a_level |= level > 0;
    }
    assert!(saw_a_level, "the hash lane still earns levels from its digest");
}
