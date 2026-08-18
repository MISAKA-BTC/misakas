//! The two ADR-0038 Decision A conjuncts that need a Layer-0 digest, over the model-free fixture.
//!
//! The other four return before the digest is computed and are unit tests in `palw_admission`.
//! These two — the happy path and the lottery — cannot be, because a digest on a PALW network is
//! an LLM inference. `MISAKA_PALW_POW_FIXTURE=1` selects the synthesized tag family instead, which
//! is exactly what it exists for: exercising the whole PALW dispatch surface without the 1.2 GB
//! pinned model. A fixture node and a real-model node compute different tags, which is correct —
//! they are different rule sets — and irrelevant here, where the subject is the admission
//! predicate rather than the tag's value.
//!
//! Its own binary because it sets a process-global environment variable.

use kaspa_consensus_core::BlueWorkType;
use kaspa_consensus_core::dns_finality::{ActiveBondView, BondStatus, STAKE_ATTESTATION_SIG_LEN, StakeBondRecord};
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::palw_block_commitment::{PALW_BLOCK_COMMITMENT_VERSION_V1, PalwBlockCommitmentV1};
use kaspa_consensus_core::palw_pwu::palw_expected_attempts_v1;
use kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_LLM;
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::{Hash64, ZERO_HASH64};
use kaspa_pow::palw_admission::{PalwAdmission, PalwAdmissionClassFacts, PalwAdmissionError, check_palw_block_admission_v1};

/// Pins no determinism class, so the fixture family is permitted and the calibration probe is a
/// no-op.
const NETWORK: &[u8] = b"devnet";

fn outpoint(seed: u64) -> TransactionOutpoint {
    TransactionOutpoint::new(Hash64::from_u64_word(seed), 0)
}

fn bonds_with(op: TransactionOutpoint) -> ActiveBondView {
    let record = StakeBondRecord {
        version: 1,
        bond_outpoint: op,
        owner_pubkey_hash: Hash64::from_u64_word(8),
        validator_pubkey_hash: Hash64::from_u64_word(7),
        validator_pubkey: vec![7u8; 32],
        amount: 20_000,
        activation_daa_score: 0,
        created_daa_score: 0,
        unbonding_period_blocks: 100,
        owner_reward_spk_payload: [0u8; 64],
        unbond_request_daa_score: None,
        slashed_at_daa_score: None,
        status: BondStatus::Active,
    };
    ActiveBondView::from_records([(op, record)])
}

fn commitment(op: TransactionOutpoint, facts: PalwAdmissionClassFacts) -> PalwBlockCommitmentV1 {
    PalwBlockCommitmentV1 {
        version: PALW_BLOCK_COMMITMENT_VERSION_V1,
        execution_class_id: Hash64::from_u64_word(0xC1),
        executor_bond_outpoint: op,
        trace_root: Hash64::from_u64_word(4),
        output_root: Hash64::from_u64_word(5),
        // The one legal value: `expected_attempts(target) × pwu_per_inference`.
        pwu_claim: palw_expected_attempts_v1(facts.class_target).saturating_mul(facts.pwu_per_inference),
        signature: vec![0x5A; STAKE_ATTESTATION_SIG_LEN],
    }
}

fn header(commitment_bytes: Vec<u8>) -> Header {
    Header::new_finalized(
        1,
        vec![vec![1.into()]].try_into().unwrap(),
        ZERO_HASH64,
        ZERO_HASH64,
        ZERO_HASH64,
        1_000_000,
        0x207fffff,
        7,
        POW_ALGO_ID_PALW_LLM,
        500,
        BlueWorkType::from_u64(0),
        0,
        ZERO_HASH64,
    )
    .with_palw_commitment(commitment_bytes)
}

fn select_the_fixture_tag_family() {
    // SAFETY: this integration test is its own binary, and both tests below read the variable
    // only through the admission call, after this has run.
    unsafe { std::env::set_var("MISAKA_PALW_POW_FIXTURE", "1") };
}

/// Every conjunct holds, and the bond that admitted the block comes back with it — so the payee
/// is the bond that acted rather than a second lookup that could resolve differently.
#[test]
fn a_complete_block_admits_and_names_its_payee() {
    select_the_fixture_tag_family();
    // The easiest possible target: the lottery admits everything, so this test is about the other
    // five conjuncts and cannot pass or fail on the ticket.
    let facts = PalwAdmissionClassFacts { class_target: u128::MAX, pwu_per_inference: 100 };
    let op = outpoint(2);
    let bonds = bonds_with(op);
    let h = header(commitment(op, facts).encode());
    match check_palw_block_admission_v1(&h, &bonds, |_| Some(facts), NETWORK, true).expect("every conjunct holds") {
        PalwAdmission::Admitted { executor_bond, commitment, ticket } => {
            assert_eq!(executor_bond.bond_outpoint, op, "the payee is the bond that acted");
            assert_eq!(commitment.executor_bond_outpoint, op);
            assert!(ticket <= facts.class_target, "an admitted block's ticket is under its class target");
        }
        other => panic!("expected admission, got {other:?}"),
    }
}

/// The lottery clause: at the hardest possible target this header's ticket misses, and the block
/// does not admit however well-formed everything else is.
#[test]
fn a_ticket_over_the_class_target_does_not_admit() {
    select_the_fixture_tag_family();
    let facts = PalwAdmissionClassFacts { class_target: 0, pwu_per_inference: 100 };
    let op = outpoint(2);
    let bonds = bonds_with(op);
    let h = header(commitment(op, facts).encode());
    match check_palw_block_admission_v1(&h, &bonds, |_| Some(facts), NETWORK, true) {
        Err(PalwAdmissionError::TicketDoesNotAdmit { ticket, class_target }) => {
            assert_eq!(class_target, 0);
            // A ticket of exactly 0 would admit at target 0 and make this vacuous. It is a
            // 128-bit digest prefix, so the odds of that are nil — asserted rather than assumed.
            assert!(ticket > 0, "the fixture drew ticket 0; this assertion is what keeps the test honest");
        }
        other => panic!("expected the lottery to refuse, got {other:?}"),
    }
}
