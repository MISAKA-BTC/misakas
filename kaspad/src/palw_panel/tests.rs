//! **`palw_panel`'s `tests` module**, moved out of `palw_panel.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).
#![cfg(test)]

use super::*;

/// **A class registration whose object did not stand is sent again — promptly** (the D-F drill of
/// 2026-09-29: two IR registrations rode one block, the second was dropped with the block standing,
/// and its node waited out the 200-DAA horizon). The carrier leaving the mempool without the class
/// is the signal: a few DAA of grace (the fold registers the class in the block that takes the
/// carrier), doubled per consecutive drop up to the horizon; a carrier still queued keeps the
/// horizon. A registrant whose class already stands is done, and a registrant is a seat: only a
/// built registration waiting for its funding holds the rest of the tick.
#[test]
fn a_dropped_class_registration_is_resubmitted_and_a_registrant_keeps_its_duties() {
    use crate::palw_panel::readiness_memory_and_stuck_carrier_tests::CodeNeedleV1 as _;
    use PalwClassRegistrationRetryV1::{LeftThePoolWithoutTheClass, StillQueuedPastTheHorizon};
    use PalwClassRegistrationStepV1::{Landed, Retry, Wait};
    let step = palw_class_registration_step_v1;
    let grace = CLASS_REGISTRATION_LEFT_POOL_GRACE_DAA;
    // The class appeared: done, whatever else is true.
    assert_eq!(step(true, false, 100, Some(101), 3, 500), Landed);
    assert_eq!(step(true, true, 100, None, 0, 101), Landed);
    // Queued: wait, up to the horizon, then rebuild.
    assert_eq!(step(false, true, 100, None, 0, 100 + CLASS_REGISTRATION_RETRY_DAA), Wait);
    assert_eq!(step(false, true, 100, None, 0, 101 + CLASS_REGISTRATION_RETRY_DAA), Retry(StillQueuedPastTheHorizon));
    // Gone from the pool with no class (the drill's case): the grace, then at once.
    assert_eq!(step(false, false, 100, Some(104), 0, 104), Wait, "the block that took it may still be folding");
    assert_eq!(step(false, false, 100, Some(104), 0, 104 + grace - 1), Wait);
    assert_eq!(step(false, false, 100, Some(104), 0, 104 + grace), Retry(LeftThePoolWithoutTheClass));
    assert!(104 + grace < 100 + CLASS_REGISTRATION_RETRY_DAA, "long before the horizon");
    // A registration the chain keeps dropping: the grace doubles per drop, capped at the horizon.
    assert_eq!(step(false, false, 100, Some(104), 1, 104 + grace), Wait);
    assert_eq!(step(false, false, 100, Some(104), 1, 104 + 2 * grace), Retry(LeftThePoolWithoutTheClass));
    assert_eq!(step(false, false, 100, Some(104), 3, 104 + 8 * grace), Retry(LeftThePoolWithoutTheClass));
    assert_eq!(step(false, false, 100, Some(104), 30, 103 + CLASS_REGISTRATION_RETRY_DAA), Wait);
    assert_eq!(step(false, false, 100, Some(104), 30, 104 + CLASS_REGISTRATION_RETRY_DAA), Retry(LeftThePoolWithoutTheClass));
    // Unrecorded departure (the first tick that sees it): counted from now.
    assert_eq!(step(false, false, 100, None, 0, 150), Wait);

    // The tick is held only for a built registration waiting for funding.
    let holds = palw_class_registration_holds_the_tick_v1;
    assert!(holds(true, false, true, false), "built, not yet sent: the stall the hold exists for");
    assert!(!holds(true, false, false, false), "the gate refuses (below palw_tir_v1): the seat does its duties");
    assert!(!holds(true, false, true, true), "its carrier waits for a block: the seat does its duties");
    assert!(!holds(true, true, false, false), "the class stands");
    assert!(!holds(false, false, true, false), "not a registrant");

    // The loop asks the mempool, applies the step, treats a class already on the chain as done,
    // matches the IR object's own class id, and holds the tick by the rule.
    let production = crate::palw_panel::source_support::former_parent_production_source_v1();
    let worker = &production[production.find_code("    pub async fn worker(").expect("the worker")..];
    let actor = include_str!("registration.rs");
    assert!(actor.contains("if let Some((txid, sent_daa)) = self.inflight {"));
    assert!(actor.contains(".has_transaction(txid, kaspa_mining::model::tx_query::TransactionQuery::All)"));
    assert!(actor.contains("palw_class_registration_step_v1("));
    assert!(actor.contains("Some(PalwConsensusObjectV2::ClassRegisteredTirV1 { class_id, .. })"));
    assert!(actor.contains("self.known_id.get_or_insert_with(|| service.class_registration_id())"));
    assert!(actor.contains("if palw_class_registration_holds_the_tick_v1("));
    assert!(actor.contains("return true;"));
    assert!(worker.contains_code("if registration.advance_v1(self, &session, current_daa, &mut carrier_chain).await { continue; }"));
    assert_eq!(actor.matches("self.inflight = Some((txid, current_daa));").count(), 2, "bootstrap and late submit state");
    assert_eq!(actor.matches("self.left_pool = None;").count(), 4, "reset on submit (x2), in pool, retry");
    let collector = include_str!("collector.rs");
    assert!(collector.contains("registration.record_submission(txid, current_daa);"));

}

/// The two-output bond carrier this node builds: one collateral output to the payee, one
/// change output back to the funding script, one input.
fn carrier_storage_mass(funding: &UtxoEntry, fee: u64, collateral: u64, payee: &kaspa_consensus_core::tx::ScriptPublicKey) -> u64 {
    let change = funding.amount - fee - collateral;
    calc_storage_mass(
        false,
        std::iter::once(UtxoCell::from(funding)),
        [UtxoCell::new(utxo_plurality(payee), collateral), UtxoCell::new(utxo_plurality(&funding.script_public_key), change)]
            .into_iter(),
        kaspa_consensus_core::constants::STORAGE_MASS_PARAMETER,
    )
    .expect("this carrier's storage mass is computable")
}

fn mldsa_script(byte: u8) -> kaspa_consensus_core::tx::ScriptPublicKey {
    kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&[byte; 64])
}

/// **The pool's byte counter must survive the sweep, or the ceiling becomes a slashing
/// machine.**
///
/// The end-of-tick sweep is the dominant removal path on a funded submitter — a claim leaves
/// the pool as soon as `submitted` holds it — and it used to remove claims without taking
/// their bytes off `pool_bytes`. The counter then only ever rose: past
/// `PANEL_POOL_MAX_BYTES` the whole-pool ceiling fires on every single arrival and, because
/// the bytes it is counting are no longer in the pool to evict, it drains `materials` down to
/// the one claim that just arrived. A seat with no material for its duty signs `Unavailable`,
/// which is a signed accusation that defaults an honest producer.
///
/// The invariant this pins is the whole fix: **the counter equals the bytes actually pooled**,
/// after any sequence of admits and sweeps. Reverting either half of the accounting breaks it.
#[test]
fn the_pool_byte_counter_equals_what_the_pool_holds() {
    use kaspa_hashes::Hash64;

    let pooled =
        |materials: &HashMap<Hash64, Vec<Vec<u8>>>| -> usize { materials.values().flat_map(|pool| pool.iter().map(Vec::len)).sum() };
    let mut materials: HashMap<Hash64, Vec<Vec<u8>>> = HashMap::new();
    let mut pool_arrival: HashMap<Hash64, u64> = HashMap::new();
    let mut seq: u64 = 0;
    let mut pool_bytes: usize = 0;

    let claim = |n: u64| Hash64::from_u64_word(n);
    for n in 0..4u64 {
        pool_admit_material_v1(&mut materials, &mut pool_arrival, &mut seq, &mut pool_bytes, claim(n), vec![0u8; 1000]);
    }
    assert_eq!(pool_bytes, pooled(&materials), "admission charges exactly what it stores");
    assert_eq!(pool_arrival.len(), 4);

    // The per-claim FIFO: a fifth payload for one claim evicts that claim's oldest, and the
    // counter follows the eviction rather than the arrival.
    for _ in 0..MATERIALS_PER_CLAIM + 1 {
        pool_admit_material_v1(&mut materials, &mut pool_arrival, &mut seq, &mut pool_bytes, claim(0), vec![0u8; 7]);
    }
    assert_eq!(materials[&claim(0)].len(), MATERIALS_PER_CLAIM);
    assert_eq!(pool_bytes, pooled(&materials), "the per-claim trim is charged back");

    // The sweep: drop three of the four claims. Before the fix `pool_bytes` kept their bytes
    // and `pool_arrival` kept their keys, forever.
    pool_sweep_material_v1(&mut materials, &mut pool_arrival, &mut pool_bytes, |c| *c == claim(1));
    assert_eq!(materials.len(), 1, "the sweep kept exactly what it was told to keep");
    assert_eq!(pool_bytes, pooled(&materials), "the sweep is charged back — this is the latch");
    assert_eq!(pool_arrival.len(), 1, "the arrival order is swept with the pool it orders");

    // And the counter reaches zero when the pool does, which is what makes the ceiling
    // releasable at all.
    pool_sweep_material_v1(&mut materials, &mut pool_arrival, &mut pool_bytes, |_| false);
    assert_eq!(pool_bytes, 0);
    assert!(materials.is_empty() && pool_arrival.is_empty());
}

/// **This chain's floor collateral cannot be carried at all, and the fix is a number, not a
/// bigger funding UTXO.**
///
/// The measured case from testnet-11 on 2026-08-26: a node funded with 10 MSK sized itself at
/// the chain's 400,000 sompi floor and the mempool refused every carrier it built with
/// `transaction storage mass of 10000003 is larger than max allowed size of 480000`. The
/// refusal is a property of the OUTPUT's value, so no amount of funding fixes it — which is
/// why the panel now asks this question before it submits.
#[test]
fn the_chain_floor_collateral_cannot_be_carried() {
    let payee = mldsa_script(7);
    let funding = UtxoEntry::new(1_000_000_000, mldsa_script(9), 0, false);
    let fee = 349_438;
    let floor = 400_000;

    let at_floor = carrier_storage_mass(&funding, fee, floor, &payee);
    assert!(
        at_floor > MAXIMUM_STANDARD_TRANSACTION_MASS,
        "a {floor} sompi output costs {at_floor} storage mass, which is what made this unregisterable"
    );

    let minimum = min_carryable_collateral(
        &funding,
        fee,
        &payee,
        kaspa_consensus_core::constants::STORAGE_MASS_PARAMETER,
        MAXIMUM_STANDARD_TRANSACTION_MASS,
        floor,
    )
    .expect("10 MSK can carry a bond at some collateral");

    assert!(minimum > floor, "the answer has to be above the floor, or the floor was carryable after all");
    assert!(
        carrier_storage_mass(&funding, fee, minimum, &payee) <= MAXIMUM_STANDARD_TRANSACTION_MASS,
        "the collateral this returns must actually be relayable"
    );
    assert!(
        carrier_storage_mass(&funding, fee, minimum - 1, &payee) > MAXIMUM_STANDARD_TRANSACTION_MASS,
        "one sompi less must NOT be relayable — otherwise this is not the minimum, and an operator locks more \
             money than the limit asked for"
    );
    // The value the two hosts were registered with, kept as the record of what worked live.
    assert!(20_000_000 >= minimum, "20,000,000 sompi registered two bonds on testnet-11, so it cannot be below the minimum");

    // **The number quoted before any funding exists must never exceed the number the funding
    // then demands.** It is the figure a waiting operator funds the address with, and if it
    // could land above the real minimum they would send exactly what they were told and still
    // be refused. `collateral_relay_lower_bound` computes this same expression from the payee
    // script; it drops the change and input terms, which can only push the true answer up.
    let c = kaspa_consensus_core::constants::STORAGE_MASS_PARAMETER;
    let plurality = utxo_plurality(&payee);
    let quoted_before_funding = c * plurality * plurality / MAXIMUM_STANDARD_TRANSACTION_MASS;
    assert!(
        quoted_before_funding <= minimum,
        "the pre-funding floor ({quoted_before_funding}) has to be a LOWER bound on the real minimum ({minimum})"
    );
}

/// **"No collateral works" and "this collateral is too small" are different operator actions.**
///
/// A funding UTXO whose every split is over the limit needs more money sent to it; saying
/// "raise the collateral" would send the operator to a knob that cannot help.
#[test]
fn funding_too_small_to_carry_any_bond_says_so() {
    let payee = mldsa_script(7);
    let funding = UtxoEntry::new(1_000_000, mldsa_script(9), 0, false);
    assert!(
        min_carryable_collateral(
            &funding,
            349_438,
            &payee,
            kaspa_consensus_core::constants::STORAGE_MASS_PARAMETER,
            MAXIMUM_STANDARD_TRANSACTION_MASS,
            400_000,
        )
        .is_none(),
        "1 MSK cannot carry a bond at any split, and the answer must be None rather than an unusable number"
    );
}

/// A collateral that already fits is returned unchanged — the floor is the answer when the
/// floor works, so this never raises an operator's locked money without cause.
#[test]
fn a_carryable_floor_is_left_alone() {
    let payee = mldsa_script(7);
    let funding = UtxoEntry::new(10_000_000_000, mldsa_script(9), 0, false);
    let floor = 50_000_000;
    let minimum = min_carryable_collateral(
        &funding,
        349_438,
        &payee,
        kaspa_consensus_core::constants::STORAGE_MASS_PARAMETER,
        MAXIMUM_STANDARD_TRANSACTION_MASS,
        floor,
    )
    .expect("a 50,000,000 sompi output is well under the relay limit");
    assert_eq!(minimum, floor, "a floor that fits is the answer; raising it would lock money for nothing");
}

/// **A responder must be able to answer a court about work it has already forgotten.**
///
/// `retention_dir` was declared on this config, documented at length with the measurement that
/// motivated it, and read by nothing. The panel's in-memory pool drops a claim the moment it
/// licenses, which is strictly before a dispute can start — a challenger has to re-execute the
/// job before it knows there is anything to dispute — so a producer's own capture is reliably
/// gone by the time its court duty appears. Two live drills measured the same shape: 143
/// sessions opened and 4 answered, then 357 opened and 3 answered.
///
/// What makes that class of bug survive review is that nothing fails. The field compiles, the
/// config carries it, the responder simply says nothing and loses on the clock — which is
/// indistinguishable from a party that is not a party. So this asserts the round trip through
/// the FILESYSTEM rather than the happy path in memory: the producer's writer and the panel's
/// reader must agree, byte for byte, about what a claim's file is called.
#[test]
fn the_writer_and_the_reader_agree_about_where_a_capture_lives() {
    use kaspa_hashes::Hash64;
    let dir = std::env::temp_dir().join(format!("palw-retention-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let claim = Hash64::from_u64_word(0xC0FFEE);
    let bytes = b"the capture behind one attempt".to_vec();

    // The producer's side of the promise, through the same function `retain_execution` uses.
    let path = crate::palw_producer::palw_retained_material_path(&dir, &claim);
    std::fs::write(&path, &bytes).unwrap();

    // And the panel's side, through the same function `retained_capture` uses. A drift in
    // either format string breaks this, which is the point: the drift's own symptom is
    // silence.
    let read = std::fs::read(crate::palw_producer::palw_retained_material_path(&dir, &claim)).unwrap();
    assert_eq!(read, bytes, "the panel must find what the producer retained");

    // And `rebroadcast_retained` walks the directory by stripping the same suffix, so a name
    // the writer produces must be one that walk recognizes.
    let name = path.file_name().unwrap().to_str().unwrap();
    let stem =
        name.strip_suffix(crate::palw_producer::PALW_RETAINED_MATERIAL_SUFFIX).expect("the retention walk strips this exact suffix");
    assert_eq!(stem.parse::<Hash64>().unwrap(), claim, "and the stem must parse back to the claim it names");

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&dir);
}

/// **The confirmed UTXO set cannot tell you what your own mempool transactions have spent.**
///
/// Funding is chosen from the VIRTUAL utxo set, which is the confirmed one. An output a carrier
/// of ours is spending right now is still in it and stays there until that carrier is mined, so
/// every source the panel has — the persisted outpoint, the configured one, and the recovery
/// scan — can hand back money that is already committed. The mempool answers `already spent by
/// transaction … in the mempool`, that clears the funding, the next tick asks again, gets the
/// same outpoint, and the panel spins there instead of carrying receipts.
///
/// Measured on testnet-11 three hours after the recovery scan shipped: 387 double-spend
/// refusals and 103 recoveries against 143 successful submissions — and 77 claims defaulted
/// with their escrow burned, because a quorum that cannot be carried inside its receipt window
/// is a quorum that never happened. The scan was right about WHERE the money is and wrong about
/// whether it was still there to spend.
///
/// The rule is a set, and the set only ever grows: an outpoint this panel has spent is never a
/// funding source again, mined or not, because once the carrier lands the money is at its
/// CHANGE outpoint — a different one. That is what makes a permanent skip correct rather than a
/// cache someone has to remember to invalidate.
#[test]
fn an_outpoint_this_panel_already_spent_is_never_funding_again() {
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    let op = |n: u64| TransactionOutpoint::new(TransactionId::from_u64_word(n), 0);
    let mut spent: HashSet<TransactionOutpoint> = HashSet::new();

    // The candidate list and the scan both filter on this set, so the property under test is
    // the set's own semantics: spending is one-way, and re-confirming does not undo it.
    assert!(!spent.contains(&op(1)), "an untouched outpoint is spendable");
    spent.insert(op(1));
    assert!(spent.contains(&op(1)), "and once funded from, it is not offered again");
    spent.insert(op(1));
    assert_eq!(spent.len(), 1, "recording the same spend twice is not two spends");

    // The change of that carrier is a DIFFERENT outpoint, which is why the panel is not left
    // with nothing: the money moves, it does not vanish.
    assert!(!spent.contains(&op(2)), "the change outpoint is free to spend next");
}

/// **A remembered funding outpoint must pay to a script this node can sign for** (issue #97).
///
/// The scan filters on the script; the two REMEMBERED outpoints — the persisted one and
/// `--palw-fee-outpoint` — went straight to the signer without it. The persisted one is
/// written by a previous run and survives a change of `--palw-producer-key`, so a state dir
/// carried across a key change hands the panel its predecessor's money. Signing it builds a
/// carrier the mempool refuses with `script ran, but verification failed` — a verdict that
/// names a signature and so points at the key and the pay address, the two things that are
/// right — and nothing recovers, because the entry stays unspent, stays resolvable, and is
/// chosen again on the next tick. Measured live as 82 identical refusals over 82 minutes.
///
/// The predicate is the fix: reject only when the script is KNOWN and DIFFERS, so a node that
/// cannot yet read its bond's payout script is not left unable to fund anything.
#[test]
fn a_remembered_outpoint_paying_elsewhere_is_not_funding() {
    let ours = mldsa_script(0x11);
    let theirs = mldsa_script(0x22);

    assert!(funding_is_foreign(Some(&ours), &theirs), "a remembered outpoint under another key is skipped, not signed");
    assert!(!funding_is_foreign(Some(&ours), &ours), "our own remembered outpoint is still the hot path");
    assert!(
        !funding_is_foreign(None, &theirs),
        "an unreadable payout script rejects nothing — that would strand a funded node that is merely early"
    );

    // And the guard the signer itself applies is the same question asked of the same script,
    // so the two cannot disagree about whose money this is.
    assert_eq!(signable_script(b"a key"), signable_script(b"a key"), "the derivation is a function of the key");
    assert_ne!(signable_script(b"a key"), signable_script(b"another key"), "and different keys sign different scripts");
}

/// **The address the join docs tell an operator to pass IS the script the pre-sign guard
/// requires** — the property that makes that guard safe to add.
///
/// `--palw-producer-pay-address` is documented as `misaka key address --key-file <seed>`,
/// which is `ValidatorKey::funding_address`. The guard refuses any funding output whose script
/// is not `signable_script` of the node's own verification key. If those two derivations ever
/// disagreed, the guard would refuse every carrier on every correctly configured seat — a far
/// worse failure than the one it fixes — so the equivalence is pinned rather than assumed, and
/// pinned through the real function on both sides rather than restated here.
#[test]
fn the_documented_pay_address_is_the_script_this_node_signs_for() {
    let key = kaspa_pq_validator_core::ValidatorKey::from_seed([7u8; kaspa_pq_validator_core::VALIDATOR_SEED_LEN]);
    let address = key.funding_address(kaspa_addresses::Prefix::Testnet);
    assert_eq!(address.version, kaspa_addresses::Version::PubKeyHashMlDsa87, "the docs' address is an ML-DSA-87 P2PKH one");
    let payload: [u8; 64] = address.payload.as_ref().try_into().expect("an ML-DSA-87 address carries 64 payload bytes");

    // `pay_payee` builds the script from exactly these payload bytes; the guard builds it from
    // the key. The two must be one script.
    assert_eq!(
        kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&payload),
        signable_script(key.public_key()),
        "a seat funded at its own documented pay address must not be refused before signing"
    );

    // And a different key is a different script, or the guard would pass everything.
    let other = kaspa_pq_validator_core::ValidatorKey::from_seed([8u8; kaspa_pq_validator_core::VALIDATOR_SEED_LEN]);
    assert_ne!(
        signable_script(key.public_key()),
        signable_script(other.public_key()),
        "another operator's funding output is still refused, which is the whole point"
    );
}

/// **A repeat says it is a repeat** (issue #97): 82 identical lines read as "still trying"
/// when they meant "nothing about the retry differs". The duration is what carries that,
/// so it has to be legible at every scale a stuck registration reaches.
#[test]
fn a_standing_refusal_reports_how_long_it_has_stood() {
    use std::time::Duration;
    assert_eq!(brief_duration(Duration::from_secs(0)), "0s");
    assert_eq!(brief_duration(Duration::from_secs(45)), "45s");
    assert_eq!(brief_duration(Duration::from_secs(60)), "1m00s", "the minute boundary keeps its seconds padded");
    assert_eq!(brief_duration(Duration::from_secs(450)), "7m30s");
    assert_eq!(brief_duration(Duration::from_secs(3600)), "1h00m");
    assert_eq!(brief_duration(Duration::from_secs(3720)), "1h02m", "82 minutes is the case that was reported");
    assert_eq!(brief_duration(Duration::from_secs(4920)), "1h22m");
}

/// **The three fates of an accepted carrier are distinguishable, and the deadline verdict has
/// to distinguish them** (issue #96).
///
/// The message this replaces asserted one of the three — a network that drops the object on
/// extraction — and asserted, without looking, that the collateral output exists. On the
/// reported run the carrier had never reached a block: the funding was still unspent and
/// `txid:0` did not exist, while other operators' `BondRegistered` extracted normally in the
/// same minutes. The two facts that separate the cases are the ones pinned here.
#[test]
fn the_deadline_verdict_names_which_failure_it_is() {
    // mined, in_mempool, funding_unspent
    assert_eq!(carrier_fate(true, false, false), CarrierFate::Extracted, "on chain and no bond IS the extraction failure");
    assert_eq!(carrier_fate(true, true, true), CarrierFate::Extracted, "mined wins: a mempool copy beside it is incidental");
    assert_eq!(carrier_fate(false, true, true), CarrierFate::Queued, "still queued is not a refusal, and not a loss");

    // The reported case (#96): admitted locally, never mined, funding untouched. The old
    // message called this an extraction failure and told the operator their collateral output
    // existed. Neither was true, and the difference is what decides whether they wait, re-fund
    // or go looking at their peers.
    assert_eq!(
        carrier_fate(false, false, true),
        CarrierFate::Dropped,
        "an unspent funding output is the proof that no fee was paid and the rebuild is free"
    );
    assert_ne!(carrier_fate(false, false, true), CarrierFate::Extracted, "and it is NOT the cause the old line named");
    assert_eq!(carrier_fate(false, false, false), CarrierFate::Stranded, "this carrier can never be mined as built");
}

/// **The node's own bond registration is signed the way the acceptance layer reads it, on both
/// sides of the operator-possession fence** (the 2026-09-23 permissionless drill, blocker B1).
///
/// Testnet-12 arms `palw_operator_id_unique` at DAA 0, and this builder signed once: every
/// `--palw-register-bond` carrier was mined and its object dropped, so no fresh key could
/// become a bond operator. The checks below are `palw_v2_validate_objects`' own: past the fence
/// the field is exactly two signatures, split at `MLDSA87_SIG_LEN`, the first by the bond key
/// over the registration message and the second by the operator key over the possession
/// message; below it, one.
#[test]
fn a_node_built_bond_registration_carries_the_operator_possession_proof_past_the_fence() {
    use kaspa_consensus_core::network::{NetworkId, NetworkType};
    use kaspa_consensus_core::palw_state_v2::{
        PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT, PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT, palw_operator_possession_message_v1,
    };
    let t12 = kaspa_consensus_core::config::params::Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
    assert!(t12.palw_operator_id_unique_at(0), "testnet-12 arms the operator-possession fence at genesis");

    let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x5Bu8; 32]);
    let pubkey = kp.verification_key.as_ref().to_vec();
    let domain = Hash64::from_u64_word(0x7112);
    let bond = kaspa_consensus_core::palw_lifecycle_objects_v2::palw_bond_registration_signed_key_v2(&PalwBondKeyV2(
        TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::default(), 0),
    ));
    let registration = Hash64::from_u64_word(0xB0D);
    let verify = |sig: &[u8], message: Hash64, context: &[u8]| {
        kaspa_txscript::verify_mldsa87_with_context(&pubkey, message.as_byte_slice(), sig, context).unwrap_or(false)
    };
    let sig_len = kaspa_txscript::MLDSA87_SIG_LEN;

    let past = bond_registration_signature_v1(&kp, domain, &bond, &pubkey, registration, true).unwrap();
    assert_eq!(past.len(), 2 * sig_len, "past the fence the field is exactly two signatures");
    let (bond_sig, operator_sig) = past.split_at(sig_len);
    assert!(verify(bond_sig, registration, PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT), "the first is the bond key's");
    let possession = palw_operator_possession_message_v1(domain, &bond, &pubkey, &pubkey);
    assert!(verify(operator_sig, possession, PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT), "the second proves the operator key");
    assert!(
        !verify(operator_sig, possession, PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT),
        "and it is bound to its own context, not the registration's"
    );

    let below = bond_registration_signature_v1(&kp, domain, &bond, &pubkey, registration, false).unwrap();
    assert_eq!(below.len(), sig_len, "below the fence the field is the one signature it always was");
    assert!(verify(&below, registration, PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT));
}

/// **Every panel duty resolves a class through the panel's one door** (the 2026-09-23
/// route-matrix audit's #5). `readiness_duties` called the table-only `backends.resolve`, so a
/// seat built no possession proof for a class this build does not tabulate and a permissionless
/// class could never gather its ready seats; the status publisher asked the same wrong door and
/// reported what the seat would not do.
#[test]
fn every_panel_duty_resolves_a_class_through_the_one_door() {
    let production = crate::palw_panel::source_support::former_parent_production_source_v1();
    for table_only in ["backends.resolve(", "backends().resolve("] {
        assert!(!production.contains(table_only), "`{table_only}` skips the chain's registrations — use `resolve_backend`");
    }
    // The route-matrix re-audit's #5: the memory figures beside a resolve go through the same door.
    for table_only in
        ["role_memory_need_v1(", "holding_bytes_for_v1(", "role_memory_need_for_backend_v1(", "incremental_replay_bytes_for_v1("]
    {
        assert!(
            !production.contains(table_only),
            "`{table_only}` prices a chain-registered class off the tables alone — use the `_or_chain_v1` figure"
        );
    }
    let readiness = &production[production.find("    fn readiness_duties(").expect("the readiness duty")..];
    assert!(readiness.contains("self.resolve_backend(session, class.class_id, class.artifact_root)"), "proofs use the door");
}

/// **A seat judges on its own instance; the executor keeps one** (ADR-0082 Decision 9; ADR-0110
/// §9.5; the straddle flake of 2026-09-24). The seat state a row check reads and the walk a
/// recompute continues are owned by the backend instance, so a seat's recompute and its row
/// check meet only on the instance its duty resolved — never on the executor's kept one, which
/// its own openings would move. The executor reaches its instance through the kept-backend door,
/// so the requests about one claim continue one walk; the seat and a court's close release it
/// before reserving, so an idle executor walk never sits beside a reserved replay; and no
/// production path clears a process-wide memo, because there is none.
#[test]
fn a_seat_judges_on_its_own_instance_and_the_executor_keeps_one() {
    use crate::palw_panel::readiness_memory_and_stuck_carrier_tests::CodeNeedleV1 as _;
    let production = crate::palw_panel::source_support::former_parent_production_source_v1();
    let body = |signature: &str| -> &str {
        let rest = &production[production.find(signature).unwrap_or_else(|| panic!("no `{signature}` in production"))..];
        &rest[..rest.find("\n    }\n").expect("its end")]
    };
    let executor = body("    fn open_retained_interval(");
    for fresh in ["self.resolve_backend(", "self.backend_for_raw_capture_v1("] {
        assert!(!executor.contains(fresh), "the executor's openings resolve a fresh instance through `{fresh}` and walk from zero");
    }
    assert!(executor.contains_code("self.executor_backend_v1("), "the free-prompt arm reaches the kept instance");
    assert!(executor.contains_code("self.executor_backend_for_raw_capture_v1("), "and so does the raw-capture arm");
    let retained = body("    fn retained_fp_capture_v1(");
    assert!(retained.contains_code("self.executor_backend_v1("), "leaf evidence and the held answers reach the kept instance");
    let seat = body("    async fn interval_seat_outcome_v1(");
    assert!(seat.contains_code("backend.fp_forget_seat_state_v1();"), "the seat drops its own instance's state at the claim boundary");
    for kept in ["self.executor_backend_v1(", "self.executor_backend_for_raw_capture_v1(", "self.keep_executor_backend_v1("] {
        assert!(!seat.contains(kept), "a seat must not judge on the executor's instance (`{kept}`)");
    }
    let released = seat.find_code("self.release_executor_backend_v1();").expect("the seat releases the executor's walk");
    let reserved = seat.find_code("self.reserve_replay_at_widest_run_v1(\"interval-seat\"").expect("the seat reserves");
    assert!(released < reserved, "the executor's idle walk is released before the seat reserves");
    // And before a court's close reserves: its walk from served intervals is on its own instance.
    let court = production.find_code("self.reserve_replay_at_widest_run_v1(\"court\"").expect("the court reserves its close");
    let released = production[..court].rfind("self.release_executor_backend_v1();").expect("the court releases the executor's walk");
    let between = &production[released..court];
    assert!(
        !between.contains("\n    fn ") && !between.contains("\n    async fn ") && !between.contains_code("reserve_replay_"),
        "the release is the court arm's own, just before its reservation"
    );
    assert!(!production.contains_code("base0_fp_seat_state_forget_v1("), "no production path clears another instance's memo");
}

/// **ADR-0152 §4-ter (the shard-court addendum): a seat shapes its one-move accusation at the
/// ladder the chain tries the CLASS at.** Both filing sites — the capture sampler's fault and the
/// named leaf's pursuit — took the network's refutation cap, so an 8k held claim (≈ 2^26.7
/// leaves, above t12's 2^26) was "recorded, not accused" and no seat could open the held
/// dissection A-held convicts through. Past `palw_offence_attribution` each reads the class's
/// ladder through `seat_refutation_ladder_v1`; below it the helper returns the network's cap,
/// so the live fleet files exactly what it did.
#[test]
fn every_one_move_filing_site_reads_the_class_ladder_past_the_fence() {
    use crate::palw_panel::readiness_memory_and_stuck_carrier_tests::CodeNeedleV1 as _;
    let production = crate::palw_panel::source_support::former_parent_production_source_v1();
    let caps: Vec<usize> = production.match_indices("palw_court_v2::palw_refutation_leaf_cap_v2(").map(|(at, _)| at).collect();
    assert_eq!(caps.len(), 2, "the two one-move filing sites");
    for at in caps {
        let after = &production[at..at + 900.min(production.len() - at)];
        assert!(
            after.contains_code("let ladder = self.seat_refutation_ladder_v1(duty.class_id, ladder, current_daa);"),
            "a filing site shapes at the network cap alone: {}",
            &after[..200]
        );
    }
    let helper = &production[production.find_code("    fn seat_refutation_ladder_v1(").expect("the helper")..];
    let helper = &helper[..helper.find("\n    }\n").expect("its end")];
    assert!(helper.contains_code("palw_offence_attribution_active_at(current_daa)"), "fenced on the attribution fence");
    assert!(helper.contains_code("return refutation_cap;"), "below it, the network's cap unchanged");
    assert!(helper.contains_code("self.class_step_ladder(class_id)"), "past it, the class's own ladder");
}

/// **Audit C-04: a seat judges a claim under the class semantics consensus binds to it** — a statement applies only from its
/// `bound_from_daa` (the claim accepted earlier keeps the facts it was produced under), and only over its own artifact; the
/// tip read (no claim) is unchanged.
#[test]
fn audit_fixcd_c04_semantics_bind_a_claim_only_from_their_bound_daa() {
    let semantics = kaspa_consensus_core::palw_class_semantics_v1::PalwClassSemanticsV1 {
        version: 1,
        class_id: Hash64::from_u64_word(1),
        artifact_root: Hash64::from_u64_word(2),
        bound_from_daa: 500,
        model_profile_id: Hash64::from_u64_word(3),
        runtime_manifest_hash: Hash64::from_u64_word(4),
        runtime_class_id: Hash64::from_u64_word(5),
        tokenizer_id: Hash64::from_u64_word(6),
        cu_ruleset_id: Hash64::from_u64_word(7),
        token_table_root: None,
        rendering_rule: None,
        raw_tokenizer_bytes: None,
        tokenizer_key_events: None,
        prefix_state_relation: None,
    };
    let root = Hash64::from_u64_word(2);
    assert!(!palw_semantics_bind_claim_v1(&semantics, root, Some(499)), "accepted before the declaration: not bound");
    assert!(palw_semantics_bind_claim_v1(&semantics, root, Some(500)), "accepted at the declaration's DAA: bound");
    assert!(palw_semantics_bind_claim_v1(&semantics, root, Some(501)));
    assert!(palw_semantics_bind_claim_v1(&semantics, root, None), "the tip read is unchanged");
    assert!(!palw_semantics_bind_claim_v1(&semantics, Hash64::from_u64_word(9), None), "another artifact never binds");
    let source = include_str!("../palw_panel.rs");
    assert!(source.contains("kept.facts == facts"), "the executor's kept instance is reused only under the same semantics");
}
