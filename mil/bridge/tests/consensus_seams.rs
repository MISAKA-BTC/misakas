//! End-to-end integration over the four consensus seams, with REAL ML-DSA-87 keys and the
//! node's own primitives throughout: beacon-bound challenge leases, bonded provider identity
//! with session-key delegation, DA obligations proved with chunk proofs, and mismatch
//! arbitration by a drawn auditor.
//!
//! Chain facts are pinned (there is no node in a unit-test environment), and the pinned bond
//! records are built FROM the generated keys — so `validator_id_from_pubkey(owner_pk)` really is
//! the registry's `owner_pubkey_hash` and every signature check is the real one. What a live
//! node adds over this harness is freshness of the beacon and of the bond status, not different
//! code paths.

use std::collections::BTreeMap;

use kaspa_consensus_core::dns_finality::validator_id_from_pubkey;
use kaspa_consensus_core::palw::da::{
    PALW_PROVIDER_SESSION_V1_MLDSA87_CONTEXT, PalwProviderSessionAuthorizationV1,
};
use kaspa_hashes::Hash64;
use kaspa_pq_validator_core::ValidatorKey;
use misaka_palw_bridge::chain::{BeaconFacts, BondFacts, ChainFacts, PinnedChainFacts};
use misaka_palw_bridge::challenge::salted_output_commitment;
use misaka_palw_bridge::da::{ChatContextObjectV4, DaCommitmentWire, DaObligationStatus, DaResponseWire};
use misaka_palw_bridge::match_key::{RUNTIME_CLASS_LABEL, bytes_hex, hash64_hex};
use misaka_palw_bridge::provider::ProviderRegistrationV1;
use misaka_palw_bridge::state::BridgeState;
use misaka_palw_bridge::wire::{JobSubmissionV1, ReplicaResultV1, RuntimeRootsV1};

const NETWORK_ID: u32 = 111;

struct Party {
    owner: ValidatorKey,
    session: ValidatorKey,
    bond_outpoint: String,
}

impl Party {
    fn new(seed_byte: u8, bond_txid_byte: u8) -> Self {
        let owner = ValidatorKey::from_seed([seed_byte; 32]);
        let session = ValidatorKey::from_seed([seed_byte.wrapping_add(128); 32]);
        Self { owner, session, bond_outpoint: format!("{}:0", format!("{bond_txid_byte:02x}").repeat(64)) }
    }

    fn credential(&self) -> Hash64 {
        validator_id_from_pubkey(self.owner.public_key())
    }

    /// A bond record as the chain would report it for this party.
    fn bond_facts(&self, operator_group: u8, amount_sompi: u64) -> BondFacts {
        BondFacts {
            bond_outpoint: self.bond_outpoint.clone(),
            owner_pubkey_hash_hex: hash64_hex(&self.credential()),
            operator_group_id_hex: format!("{operator_group:02x}").repeat(64),
            amount_sompi,
            activation_daa_score: 10,
            effective_status: "active".into(),
            unbond_request_daa_score: None,
            slashed_at_daa_score: None,
            unbond_delay_epochs: 6,
            reward_key_root_hex: "00".repeat(64),
            runtime_classes_hex: vec![],
            capacity_by_shape: vec![],
        }
    }

    /// A genuinely signed session delegation (the node's own object, the node's own context).
    fn registration(&self, valid_from: u64, valid_until: u64) -> ProviderRegistrationV1 {
        let mut auth = PalwProviderSessionAuthorizationV1 {
            version: 1,
            network_id: NETWORK_ID,
            provider_bond: misaka_palw_bridge::chain::parse_outpoint(&self.bond_outpoint).unwrap(),
            owner_public_key: self.owner.public_key().to_vec(),
            session_public_key: self.session.public_key().to_vec(),
            valid_from_epoch: valid_from,
            valid_until_epoch: valid_until,
            authorization_nonce: Hash64::from_bytes([9u8; 64]),
            signature: Vec::new(),
        };
        let signature = self.owner.sign_with_context(auth.signing_hash().as_byte_slice(), PALW_PROVIDER_SESSION_V1_MLDSA87_CONTEXT);
        auth.signature = signature.to_vec();
        ProviderRegistrationV1 {
            bond_outpoint: self.bond_outpoint.clone(),
            owner_public_key_hex: bytes_hex(self.owner.public_key()),
            session_authorization_hex: bytes_hex(&borsh::to_vec(&auth).unwrap()),
        }
    }
}

fn beacon() -> BeaconFacts {
    BeaconFacts {
        epoch: 12,
        seed_hex: "ab".repeat(64),
        anchor_hash_hex: "cd".repeat(64),
        anchor_daa_score: 1_200,
        observed_daa_score: 1_500,
        current_epoch: 15,
    }
}

fn roots(route: &str) -> RuntimeRootsV1 {
    RuntimeRootsV1 { route: route.into(), kv: "bb22".into(), state: "cc33".into() }
}

fn output_root_hex(ids: &[u32]) -> String {
    // The gateway's blake2b-256 over LE u32s (protocol v1's `output_root`).
    let mut h = blake2b_simd::Params::new().hash_length(32).to_state();
    for id in ids {
        h.update(&id.to_le_bytes());
    }
    h.finalize().as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

struct Harness {
    state: BridgeState,
    chain: PinnedChainFacts,
    a: Party,
    b: Party,
    auditor: Party,
}

fn harness(name: &str) -> Harness {
    let dir = std::env::temp_dir().join(format!("palw-bridge-seams-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let a = Party::new(1, 0x11);
    let b = Party::new(2, 0x22);
    let auditor = Party::new(3, 0x33);

    let mut bonds = BTreeMap::new();
    bonds.insert(a.bond_outpoint.clone(), a.bond_facts(1, 1_000_000_000));
    bonds.insert(b.bond_outpoint.clone(), b.bond_facts(2, 1_000_000_000));
    // The auditor is deliberately given a SMALLER stake so the "exclusion beats weight" and
    // "only unconflicted candidate" properties are not accidentally satisfied by weight alone.
    bonds.insert(auditor.bond_outpoint.clone(), auditor.bond_facts(3, 500_000_000));

    let chain = PinnedChainFacts::from_parts(beacon(), bonds);
    let state = BridgeState::open(&dir, 120_000, NETWORK_ID).unwrap();
    let mut h = Harness { state, chain, a, b, auditor };
    for party in [&h.a, &h.b, &h.auditor] {
        let registration = party.registration(0, 100);
        h.state.register_provider(&registration, &h.chain, 1_000).unwrap();
    }
    h
}

/// Seam 2: registration is a real chain + signature check, not a self-declared label.
#[test]
fn bonded_registration_requires_the_real_key_and_an_active_bond() {
    let mut h = harness("registration");
    assert!(h.state.registered_provider(&h.a.bond_outpoint).is_some());

    // Someone else's key against A's bond: the credential hash will not match.
    let impostor = Party { owner: ValidatorKey::from_seed([9u8; 32]), session: ValidatorKey::from_seed([10u8; 32]), bond_outpoint: h.a.bond_outpoint.clone() };
    let err = h.state.register_provider(&impostor.registration(0, 100), &h.chain, 1_001).unwrap_err();
    assert!(err.contains("does not hash to the bond"), "{err}");

    // A tampered session signature is refused by the node's own verifier.
    let mut registration = h.b.registration(0, 100);
    let mut auth: PalwProviderSessionAuthorizationV1 =
        borsh::from_slice(&misaka_palw_bridge::match_key::decode_hex(&registration.session_authorization_hex).unwrap()).unwrap();
    auth.signature[0] ^= 0xff;
    registration.session_authorization_hex = bytes_hex(&borsh::to_vec(&auth).unwrap());
    let err = h.state.register_provider(&registration, &h.chain, 1_002).unwrap_err();
    assert!(err.contains("signature"), "{err}");

    // An expired delegation window is refused (current epoch is 15).
    let err = h.state.register_provider(&h.b.registration(0, 5), &h.chain, 1_003).unwrap_err();
    assert!(err.contains("covers epochs"), "{err}");

    // A slashed bond cannot register at all.
    let mut bonds = BTreeMap::new();
    let mut slashed = h.a.bond_facts(1, 1_000_000_000);
    slashed.effective_status = "slashed".into();
    slashed.slashed_at_daa_score = Some(20);
    bonds.insert(h.a.bond_outpoint.clone(), slashed);
    let hostile_chain = PinnedChainFacts::from_parts(beacon(), bonds);
    let err = h.state.register_provider(&h.a.registration(0, 100), &hostile_chain, 1_004).unwrap_err();
    assert!(err.contains("slashed"), "{err}");
}

/// Seam 1: the challenge is leased BEFORE generation and binds the prompt, so a provider cannot
/// regenerate and re-commit under a challenge that fits the answer it liked.
#[test]
fn challenge_lease_binds_the_prompt_and_defeats_grinding() {
    let mut h = harness("lease");
    let prompt = vec![1u32, 2, 3];
    let lease = h
        .state
        .lease_challenge(&h.a.bond_outpoint, &prompt, 256, RUNTIME_CLASS_LABEL, 1, &h.chain, 2_000)
        .unwrap();
    lease.verify_self_consistent().unwrap();
    assert_eq!(lease.beacon_epoch, 12, "bound to the buried beacon sample");
    assert_eq!(lease.beacon_seed_hex, "ab".repeat(64));

    // Same inputs ⇒ the SAME challenge (idempotent; there is no re-roll).
    let again = h
        .state
        .lease_challenge(&h.a.bond_outpoint, &prompt, 256, RUNTIME_CLASS_LABEL, 1, &h.chain, 2_001)
        .unwrap();
    assert_eq!(lease.job_challenge_hex, again.job_challenge_hex);

    // A different prompt ⇒ a different challenge, and neither lease accepts the other's prompt.
    let other = h
        .state
        .lease_challenge(&h.a.bond_outpoint, &[9u32, 9, 9], 256, RUNTIME_CLASS_LABEL, 1, &h.chain, 2_002)
        .unwrap();
    assert_ne!(lease.job_challenge_hex, other.job_challenge_hex);
    assert!(other.accepts(&prompt, 256, RUNTIME_CLASS_LABEL, &h.a.credential(), 15).is_err());

    // A different provider gets a different challenge for the SAME prompt (leases are not
    // transferable).
    let b_lease = h
        .state
        .lease_challenge(&h.b.bond_outpoint, &prompt, 256, RUNTIME_CLASS_LABEL, 1, &h.chain, 2_003)
        .unwrap();
    assert_ne!(lease.job_challenge_hex, b_lease.job_challenge_hex);
    assert!(b_lease.accepts(&prompt, 256, RUNTIME_CLASS_LABEL, &h.a.credential(), 15).is_err());
}

/// Seam 1 (cont.): the submission must carry the leased challenge AND the salted receipt-v3
/// output commitment over the ids it claims.
#[test]
fn submission_must_match_the_lease_and_the_salted_commitment() {
    let mut h = harness("submit");
    let prompt = vec![1u32, 2, 3];
    let output = vec![10u32, 20, 30];
    let lease = h
        .state
        .lease_challenge(&h.a.bond_outpoint, &prompt, 256, RUNTIME_CLASS_LABEL, 1, &h.chain, 2_000)
        .unwrap();
    let challenge = lease.job_challenge().unwrap();

    let good = JobSubmissionV1 {
        job_id: "job-1".into(),
        provider_id: h.a.bond_outpoint.clone(),
        prompt_ids: prompt.clone(),
        max_new: 256,
        output_root: output_root_hex(&output),
        receipt_json: None,
        runtime_roots: Some(roots("aa11")),
        job_challenge: Some(lease.job_challenge_hex.clone()),
        output_token_ids: Some(output.clone()),
        output_commitment: Some(hash64_hex(&salted_output_commitment(&output, &challenge))),
    };
    h.state.check_lease(&good, &h.a.bond_outpoint, RUNTIME_CLASS_LABEL, 15).unwrap();

    // A commitment over DIFFERENT output ids is refused — this is the anti-grinding bite: the
    // provider cannot swap the answer while keeping the leased challenge.
    let swapped = JobSubmissionV1 { output_token_ids: Some(vec![99, 99]), ..good.clone() };
    assert!(h.state.check_lease(&swapped, &h.a.bond_outpoint, RUNTIME_CLASS_LABEL, 15).is_err());

    // An unleased challenge is refused.
    let forged = JobSubmissionV1 { job_challenge: Some("ff".repeat(64)), ..good.clone() };
    assert!(h.state.check_lease(&forged, &h.a.bond_outpoint, RUNTIME_CLASS_LABEL, 15).is_err());

    // A prompt that differs from the leased one is refused.
    let other_prompt = JobSubmissionV1 { prompt_ids: vec![7, 7], ..good.clone() };
    assert!(h.state.check_lease(&other_prompt, &h.a.bond_outpoint, RUNTIME_CLASS_LABEL, 15).is_err());

    // An expired lease is refused (lease covers epochs 15..=21).
    assert!(h.state.check_lease(&good, &h.a.bond_outpoint, RUNTIME_CLASS_LABEL, 99).is_err());

    // …and B cannot use A's lease.
    assert!(h.state.check_lease(&good, &h.b.bond_outpoint, RUNTIME_CLASS_LABEL, 15).is_err());
}

/// Seam 3: obligations are beacon-sampled, provable with a real chunk proof, and a silent
/// provider produces timeout evidence naming its bond.
#[test]
fn da_obligation_is_sampled_proved_and_can_time_out() {
    let mut h = harness("da");
    let object = ChatContextObjectV4 {
        network_id: NETWORK_ID,
        job_challenge: Hash64::from_bytes([7u8; 64]),
        class_label: RUNTIME_CLASS_LABEL.to_vec(),
        max_new: 256,
        // Big enough to span several 16 KiB chunks, so the sample is a real choice.
        prompt_token_ids: (0..20_000).collect(),
        output_token_ids: vec![10, 20, 30],
    };
    let bytes = object.encode().unwrap();
    let commitment = DaCommitmentWire::from_commitment(&object.commitment().unwrap());
    assert!(commitment.chunk_count >= 4, "want a multi-chunk object, got {}", commitment.chunk_count);

    let obligations = h.state.register_da("job-1", &h.a.bond_outpoint, &commitment, &h.chain, 3_000).unwrap();
    assert_eq!(obligations.len(), 1);
    let obligation = obligations[0].clone();

    // Nothing to answer until challenged.
    let response = DaResponseWire::prove(&obligation, &bytes).unwrap();
    assert!(h.state.answer_da_challenge(&response, &h.chain, 3_001).is_err());

    let opened = h.state.open_da_challenges(&h.a.bond_outpoint, &h.chain, 3_002).unwrap();
    assert_eq!(opened.len(), 1);
    assert!(matches!(opened[0].status, DaObligationStatus::Challenged { .. }));

    // The honest provider answers with the sampled chunk and its Merkle path.
    h.state.answer_da_challenge(&response, &h.chain, 3_003).unwrap();
    assert_eq!(h.state.da_obligations_for(&h.a.bond_outpoint)[0].status, DaObligationStatus::Satisfied);

    // A tampered chunk is refused even now that the obligation is Satisfied — verification runs
    // BEFORE the idempotent short-circuit, so "already satisfied" can never launder a bad proof.
    let mut tampered = DaResponseWire::prove(&obligation, &bytes).unwrap();
    let mut chunk = misaka_palw_bridge::match_key::decode_hex(&tampered.chunk_hex).unwrap();
    chunk[0] ^= 0xff;
    tampered.chunk_hex = bytes_hex(&chunk);
    assert!(tampered.verify(&obligation).is_err(), "the proof itself must not verify");
    assert!(
        h.state.answer_da_challenge(&tampered, &h.chain, 3_004).is_err(),
        "a bad proof against a satisfied obligation must still be refused"
    );
    // …while an honest re-send stays idempotent.
    h.state.answer_da_challenge(&response, &h.chain, 3_005).unwrap();

    // A provider that never answers: sweeping past the deadline produces timeout evidence.
    let mut silent = harness("da-timeout");
    silent.state.register_da("job-2", &silent.b.bond_outpoint, &commitment, &silent.chain, 3_000).unwrap();
    silent.state.open_da_challenges(&silent.b.bond_outpoint, &silent.chain, 3_001).unwrap();
    // Advance the chain past the response window (STRICT_TESTNET: 200 DAA).
    let mut later = beacon();
    later.observed_daa_score += 10_000;
    let mut bonds = BTreeMap::new();
    bonds.insert(silent.b.bond_outpoint.clone(), silent.b.bond_facts(2, 1_000_000_000));
    let later_chain = PinnedChainFacts::from_parts(later, bonds);
    let timed_out = silent.state.sweep_da_timeouts(&later_chain, 3_500).unwrap();
    assert_eq!(timed_out.len(), 1);
    assert_eq!(silent.state.da_obligations_for(&silent.b.bond_outpoint)[0].status, DaObligationStatus::TimedOut);
}

/// Seam 4: a k=2 mismatch opens a dispute, escalates through the real draw, draws an
/// unconflicted auditor, and the auditor's reference run attributes the slash.
#[test]
fn mismatch_is_arbitrated_by_a_drawn_auditor() {
    let mut h = harness("dispute");
    let prompt = vec![1u32, 2, 3];
    let a_output = vec![10u32, 20, 30];
    let submission = JobSubmissionV1 {
        job_id: "job-1".into(),
        provider_id: h.a.bond_outpoint.clone(),
        prompt_ids: prompt.clone(),
        max_new: 256,
        output_root: output_root_hex(&a_output),
        receipt_json: None,
        runtime_roots: Some(roots("aa11")),
        job_challenge: None,
        output_token_ids: Some(a_output.clone()),
        output_commitment: None,
    };
    h.state.submit_job(&submission, 4_000).unwrap();
    let assignments = h.state.fetch_assignments(&h.b.bond_outpoint, 4_001).unwrap();
    assert_eq!(assignments.len(), 1, "the submitter is never offered its own job");

    // B disagrees (different output entirely).
    let b_output = vec![99u32];
    let matched = h
        .state
        .submit_replica_result(
            &ReplicaResultV1 {
                job_id: "job-1".into(),
                provider_id: h.b.bond_outpoint.clone(),
                output_root: output_root_hex(&b_output),
                runtime_roots: Some(roots("aa11")),
            },
            4_002,
        )
        .unwrap();
    assert!(!matched);

    let dispute = h.state.open_dispute("job-1", &h.chain, 4_003).unwrap().expect("a dispute record");
    assert!(dispute.escalated, "the bridge policy escalates every real mismatch");
    let auditor = dispute.auditor.clone().expect("an auditor was drawn");
    assert_eq!(auditor, h.auditor.bond_outpoint, "neither disputant may adjudicate its own dispute");

    // The auditor sees the job it must replay.
    let audits = h.state.audit_assignments_for(&auditor);
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].1, prompt, "the auditor replays the disputed prompt");

    // Nobody else can answer for that dispute.
    assert!(
        h.state
            .adjudicate_dispute(&dispute.dispute_id_hex, &h.b.bond_outpoint, &output_root_hex(&b_output), &roots("aa11"), 4_004)
            .is_err(),
        "a disputant cannot adjudicate"
    );

    // The auditor's reference run agrees with A ⇒ B is the slash target.
    let evidence = h
        .state
        .adjudicate_dispute(&dispute.dispute_id_hex, &auditor, &output_root_hex(&a_output), &roots("aa11"), 4_005)
        .unwrap();
    assert_eq!(evidence.verdict, "slash_b");
    assert_eq!(evidence.slash_targets, vec![h.b.bond_outpoint.clone()]);
    assert_eq!(evidence.auditor, auditor);
    assert!(!evidence.journal_root_hex.is_empty(), "evidence is anchored to a journal position");

    // The whole run survives a restart with the same head root, disputes and all.
    let head = h.state.head_root_hex();
    let dir = std::env::temp_dir().join(format!("palw-bridge-seams-{}-dispute", std::process::id()));
    drop(h);
    let reopened = BridgeState::open(&dir, 120_000, NETWORK_ID).unwrap();
    assert_eq!(reopened.head_root_hex(), head);
    assert_eq!(reopened.disputes_json().len(), 1);
}

/// The reference run agreeing with NEITHER side slashes both — the SlashBoth arm.
#[test]
fn reference_run_agreeing_with_neither_side_slashes_both() {
    let mut h = harness("slashboth");
    let submission = JobSubmissionV1 {
        job_id: "job-1".into(),
        provider_id: h.a.bond_outpoint.clone(),
        prompt_ids: vec![1, 2, 3],
        max_new: 256,
        output_root: output_root_hex(&[10, 20]),
        receipt_json: None,
        runtime_roots: Some(roots("aa11")),
        job_challenge: None,
        output_token_ids: Some(vec![10, 20]),
        output_commitment: None,
    };
    h.state.submit_job(&submission, 5_000).unwrap();
    h.state.fetch_assignments(&h.b.bond_outpoint, 5_001).unwrap();
    h.state
        .submit_replica_result(
            &ReplicaResultV1 {
                job_id: "job-1".into(),
                provider_id: h.b.bond_outpoint.clone(),
                output_root: output_root_hex(&[30, 40]),
                runtime_roots: Some(roots("aa11")),
            },
            5_002,
        )
        .unwrap();
    let dispute = h.state.open_dispute("job-1", &h.chain, 5_003).unwrap().unwrap();
    let auditor = dispute.auditor.clone().unwrap();
    let evidence = h
        .state
        .adjudicate_dispute(&dispute.dispute_id_hex, &auditor, &output_root_hex(&[77]), &roots("aa11"), 5_004)
        .unwrap();
    assert_eq!(evidence.verdict, "slash_both");
    assert_eq!(evidence.slash_targets.len(), 2);
}

/// With no unconflicted third party, the dispute stays open rather than being adjudicated by
/// someone with a stake in the answer.
#[test]
fn no_unconflicted_auditor_leaves_the_dispute_open() {
    let dir = std::env::temp_dir().join(format!("palw-bridge-seams-{}-noauditor", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let a = Party::new(1, 0x11);
    let b = Party::new(2, 0x22);
    let mut bonds = BTreeMap::new();
    bonds.insert(a.bond_outpoint.clone(), a.bond_facts(1, 1_000_000_000));
    bonds.insert(b.bond_outpoint.clone(), b.bond_facts(2, 1_000_000_000));
    let chain = PinnedChainFacts::from_parts(beacon(), bonds);
    let mut state = BridgeState::open(&dir, 120_000, NETWORK_ID).unwrap();
    for party in [&a, &b] {
        state.register_provider(&party.registration(0, 100), &chain, 1_000).unwrap();
    }

    state
        .submit_job(
            &JobSubmissionV1 {
                job_id: "job-1".into(),
                provider_id: a.bond_outpoint.clone(),
                prompt_ids: vec![1],
                max_new: 8,
                output_root: output_root_hex(&[1]),
                receipt_json: None,
                runtime_roots: Some(roots("aa11")),
                job_challenge: None,
                output_token_ids: Some(vec![1]),
                output_commitment: None,
            },
            6_000,
        )
        .unwrap();
    state.fetch_assignments(&b.bond_outpoint, 6_001).unwrap();
    state
        .submit_replica_result(
            &ReplicaResultV1 {
                job_id: "job-1".into(),
                provider_id: b.bond_outpoint.clone(),
                output_root: output_root_hex(&[2]),
                runtime_roots: Some(roots("aa11")),
            },
            6_002,
        )
        .unwrap();
    let dispute = state.open_dispute("job-1", &chain, 6_003).unwrap().unwrap();
    assert!(dispute.escalated);
    assert!(dispute.auditor.is_none(), "only the two disputants exist — nobody may adjudicate");
    assert!(dispute.verdict.is_none());
}

/// The chain-facts source is always reported, so an operator can tell a live verdict from a
/// pinned-number one.
#[test]
fn pinned_facts_are_reported_as_not_live() {
    let h = harness("label");
    assert!(!h.chain.is_live());
    assert!(h.chain.source_label().contains("NOT live"));
}
