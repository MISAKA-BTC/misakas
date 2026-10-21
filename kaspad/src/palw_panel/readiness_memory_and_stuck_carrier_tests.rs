//! **`palw_panel`'s `readiness_memory_and_stuck_carrier_tests` module**, moved out of `palw_panel.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).
#![cfg(test)]

use super::*;
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_readiness_escalation_v1::PALW_READINESS_ESCALATION_LANDING_DAA_V1;

/// Match code shapes across rustfmt line wrapping while returning original source offsets.
pub(super) trait CodeNeedleV1 {
    fn find_code(&self, needle: &str) -> Option<usize>;
    fn contains_code(&self, needle: &str) -> bool {
        self.find_code(needle).is_some()
    }
    fn count_code(&self, needle: &str) -> usize;
}
fn code_span_v1(source: &str, needle: &str) -> Option<std::ops::Range<usize>> {
    // Known call-shape guards only: not encoded-value or general Rust AST equality.
    let compact = |text: &str| {
        let positions: Vec<usize> = text.bytes().enumerate().filter_map(|(at, b)| (!b.is_ascii_whitespace()).then_some(at)).collect();
        positions
            .iter()
            .enumerate()
            .filter_map(|(at, original)| {
                let trailing_argument_comma =
                    text.as_bytes()[*original] == b',' && positions.get(at + 1).is_some_and(|next| text.as_bytes()[*next] == b')');
                (!trailing_argument_comma).then_some((*original, text.as_bytes()[*original]))
            })
            .collect::<Vec<_>>()
    };
    let haystack = compact(source);
    let wanted: Vec<u8> = compact(needle).into_iter().map(|(_, byte)| byte).collect();
    if wanted.is_empty() {
        return Some(0..0);
    }
    haystack
        .windows(wanted.len())
        .position(|window| window.iter().map(|(_, byte)| byte).copied().eq(wanted.iter().copied()))
        .map(|at| haystack[at].0..haystack[at + wanted.len() - 1].0 + 1)
}
impl CodeNeedleV1 for str {
    fn find_code(&self, needle: &str) -> Option<usize> {
        code_span_v1(self, needle).map(|span| span.start)
    }
    fn count_code(&self, needle: &str) -> usize {
        let mut rest = self;
        let mut count = 0;
        while let Some(span) = code_span_v1(rest, needle) {
            if span.is_empty() {
                break;
            }
            count += 1;
            rest = &rest[span.end..];
        }
        count
    }
}

fn production() -> &'static str {
    let whole = include_str!("../palw_panel.rs");
    &whole[..whole.find("#[cfg(test)]\nmod tests").expect("the test module")]
}

/// The drill's A16 geometry, derived in-tree — the seat's own prover path runs on it.
fn a16_fixture() -> (misaka_palw_base0::qwen25_a16_backend::Qwen25A16Backend, Hash64) {
    let geometry = kaspa_consensus_core::palw_e2e_adjudicability::PALW_RC_A16_DRILL_GEOMETRY;
    let shape = misaka_palw_base0::artifact::Base0ShapeV1 {
        n_layers: geometry.layer_count as usize,
        n_heads: geometry.attn_heads as usize,
        n_kv_heads: geometry.attn_kv_heads as usize,
        d_head: geometry.attn_head_dim as usize,
        d_ff: geometry.ffn_dim as usize,
        vocab: geometry.vocab_size as usize,
        max_position: geometry.n_ctx as usize,
        ln_theta_gen_q: misaka_palw_base0::artifact::LN_THETA_10000_GEN_Q,
        eps_q: kaspa_consensus_core::palw_qwen25_profile::QWEN25_A16_ARTIFACT_EPS_Q,
    };
    let artifact = misaka_palw_base0::artifact::Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
        .unwrap()
        .with_a16_params(misaka_palw_base0::engine_a16::derived_a16_store(&shape))
        .unwrap();
    let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v5(geometry).unwrap();
    let class_id = profile.shape_profile_id();
    let backend = misaka_palw_base0::qwen25_a16_backend::Qwen25A16Backend::from_registered_profile(
        std::sync::Arc::new(artifact),
        b"misaka-palw-rc".to_vec(),
        profile,
        (4, 2),
    )
    .unwrap();
    (backend, class_id)
}

/// **(a) reproduced and held: a 3.5 GiB seat proves the 8k class while a replay holds its share.**
/// The seat's share is 3,584 MiB and the 8k full seat 3,456 MiB. With the floor's replay (as it
/// was priced, 2.18 GiB) or the 8k's own in flight, the old gate — a second full seat, now —
/// refuses: the drill's `no proof — … 0.13 GiB available`. The new path asks the host's capacity,
/// takes the proof's own bytes from the carve, and builds the proof STREAMED through the backend's
/// own walk — the very proof the leaf-vector builder makes, verified against the root — while the
/// ledger never holds more than the share.
#[test]
fn the_8k_proof_is_built_streamed_beside_a_replay_on_a_three_and_a_half_gib_seat() {
    use crate::palw_memory_ledger::{
        PALW_READINESS_PROOF_CARVE_BYTES_V1, PALW_READINESS_PROOF_ROLE_V1, PalwMemoryLedgerV1, PalwMemoryPoolV1,
        PalwMemoryReservationKeyV1,
    };
    use kaspa_consensus_core::palw_artifact::{
        PalwArtifactMultiproofStreamV1, palw_artifact_multiproof_v1, verify_artifact_multiproof_v1,
    };
    use kaspa_consensus_core::palw_model_registry_v1::{
        PALW_READINESS_V2_BUDGET_BYTES_V1, palw_readiness_v2_challenge_seed_v1, palw_readiness_v2_draw_v1,
    };
    const MIB: u64 = 1 << 20;
    let (share, full_8k) = (3_584 * MIB, 3_456 * MIB);
    let carve = PALW_READINESS_PROOF_CARVE_BYTES_V1;
    let ledger = PalwMemoryLedgerV1::new_with_proof_carve(PalwMemoryPoolV1::Host, Some(share), carve, || None);
    let (backend, class_id) = a16_fixture();
    let (root, leaf_count) = backend.artifact_root_and_leaf_count().unwrap();
    let bond = borsh::to_vec(&PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(0xB2), 0))).unwrap();
    for (in_flight, what) in [(2_232 * MIB, "the floor's replay as the drill priced it"), (full_8k, "the 8k's own replay")] {
        let replay_key =
            PalwMemoryReservationKeyV1 { role: "full-seat", class_id: Hash64::from_u64_word(1), job: Hash64::from_u64_word(1) };
        let replay = ledger.reserve(replay_key, in_flight).unwrap_or_else(|e| panic!("{what}: {e}"));
        assert!(ledger.can_reserve(full_8k).is_err(), "{what}: the drill's gate proves nothing");
        ledger.capacity_admits(full_8k).unwrap_or_else(|e| panic!("{what}: capacity: {e}"));
        for span in [26u64, 27, 28] {
            let held = ledger
                .reserve(
                    PalwMemoryReservationKeyV1 { role: PALW_READINESS_PROOF_ROLE_V1, class_id, job: Hash64::from_u64_word(span) },
                    PALW_READINESS_PROOF_CARVE_BYTES_V1,
                )
                .unwrap_or_else(|e| panic!("{what} span {span}: the proof's bytes: {e}"));
            assert!(ledger.reserved_bytes() <= share, "{what}: never past the share");
            let draw = palw_readiness_v2_draw_v1(&palw_readiness_v2_challenge_seed_v1(&class_id, &bond, span), leaf_count);
            let mut stream = PalwArtifactMultiproofStreamV1::new(leaf_count, &draw).expect("the draw is inside the inventory");
            let (streamed_root, streamed_count, drawn) = backend
                .artifact_readiness_material_streamed_v1(&draw, &mut |leaf| stream.push(leaf))
                .expect("the A16 family streams")
                .expect("the walk succeeds");
            assert_eq!((streamed_root, streamed_count), (root, leaf_count));
            // The seat's budget, in the draw's order — `readiness_duties` exactly.
            let (mut opened, mut bytes) = (Vec::new(), 0usize);
            for (index, operand) in drawn.clone() {
                if bytes >= PALW_READINESS_V2_BUDGET_BYTES_V1 {
                    break;
                }
                bytes += operand.bytes.len();
                opened.push((index, operand));
            }
            opened.sort_by_key(|(index, _)| *index);
            let proof = stream.finish(&opened).expect("the stream proves");
            let (_, leaves, material) = backend.artifact_readiness_material(&draw).unwrap();
            assert_eq!(material, drawn, "the same walk, the same drawn rows");
            assert_eq!(Some(&proof), palw_artifact_multiproof_v1(&leaves, &opened).as_ref(), "the builder's proof, byte for byte");
            verify_artifact_multiproof_v1(&proof, root).expect("and it opens the registered root");
            drop(held);
        }
        drop(replay);
    }
    // A class this share can never replay is still refused — capacity is not a waiver.
    assert!(ledger.capacity_admits(share).is_err(), "past the share less the carve");
}

/// **The panel asks these questions, in this order** (a): capacity — never the moment — before a
/// proof; the proof lane's reservation before the walk; the streamed walk first; and the floor's
/// figure without another class's holding (`palw_backends`).
#[test]
fn the_readiness_duty_asks_capacity_and_takes_its_bytes_from_the_proof_lane() {
    let source = production();
    let duties = &source[source.find("    fn readiness_duties(").expect("readiness_duties")..];
    let duties = &duties[..duties.find("\n    }\n").expect("its end")];
    assert!(duties.contains("self.replay_memory_capacity_v1(session, Some((class.class_id, class.artifact_root)))"));
    assert!(!duties.contains("replay_memory_budget_v1("), "the moment's gate starved the drill's proofs");
    let reserve = duties.find("self.reserve_readiness_proof_v1(class.class_id, span_now, streamed_need)").expect("the proof lane");
    let streamed = duties.find("backend.artifact_readiness_material_streamed_v1(").expect("the streamed walk");
    let vector = duties.find("backend.artifact_readiness_material(&draw)").expect("the vector, for a family that cannot stream");
    assert!(reserve < streamed && streamed < vector, "reserve, stream, and only then the vector");
    assert!(duties.contains("palw_readiness_proof_materialized_need_v1(leaf_count)"), "the vector is priced before it is built");
    // The status says what the seat does.
    let status = &source[source.find("    fn publish_local_panel_status(").expect("the status")..];
    let status = &status[..status.find("\n    }\n").expect("its end")];
    assert!(status.contains("self.replay_memory_capacity_v1(session, Some((class.class_id, class.artifact_root)))"));
    // The seat's REPLAY still asks the moment, and still reserves when it starts.
    assert!(source.contains("self.replay_memory_budget_v1(&session, Some((duty.class_id, duty.artifact_root)))"));
    let backends = include_str!("../palw_backends.rs");
    assert!(backends.contains("fn serves_without_a_holding_v1("), "the derived floor is priced without a holding");
    // A vector build is priced at four vectors' worth of hashes plus the streamed figure.
    assert_eq!(
        palw_readiness_proof_materialized_need_v1(649_480),
        649_480 * 64 * 4 + crate::palw_memory_ledger::PALW_READINESS_PROOF_CARVE_BYTES_V1
    );
}

/// **RFC-0004 §6.7 (spec 17 §17.7.1): a composite is possessed over its adapter section, and only against the chain's
/// record.** The duty asks the backend for its possession tree before it draws, reads the chain's composite records for a
/// composite (a class the chain holds none of, or whose record names another section, is not proved: the fold would refuse
/// the proof), draws over the section's own leaf count and checks the proof against the section's root — never against a
/// composite's registered root, which no multiproof opens.
#[test]
fn the_readiness_duty_proves_a_composite_over_its_adapter_section_against_the_chains_record() {
    let source = production();
    let duties = &source[source.find("    fn readiness_duties(").expect("readiness_duties")..];
    let duties = &duties[..duties.find("\n    }\n").expect("its end")];
    let tree = duties.find("backend.artifact_possession_tree_v1()").expect("the possession tree");
    let records = duties.find("session.palw_improvement_status_v1().composite_classes").expect("the chain's composite records");
    let draw = duties.find("palw_readiness_v2_draw_v1(&seed_v2, leaf_count)").expect("the draw");
    assert!(tree < records && records < draw, "possession tree, then the record, then the draw over the section's leaves");
    assert!(
        duties.contains("record.adapter_root == section_root && record.artifact_root() == class.artifact_root"),
        "the record names this section and this class"
    );
    assert!(
        duties.contains("verify_artifact_multiproof_v1(&proof, possession_root)"),
        "the proof is checked against the section's root"
    );
    assert!(
        !duties.contains("verify_artifact_multiproof_v1(&proof, class.artifact_root)"),
        "never against a composite's registered root"
    );
}

/// **(b) reproduced and held: a carrier stuck in our own mempool yields to the proof inside the
/// escalation margin.** Testnet-12's horizon: a row proved at 100 is due past 112 (the Own site),
/// escalates from 122 and counts through 124. A carrier of ours sent at `stuck_at` never lands —
/// the drill's shape, a duplicate the fold stopped taking, which only a template build evicts and
/// a seat that mines nothing never builds. The old tick sends nothing while it holds the slot and
/// the row lapses at 125 whatever `stuck_at` was. The new tick replaces it: an escalated proof
/// once the carrier is late, a merely-due proof once it is lost — and for every carrier stuck
/// before the escalation's own landing margin, the proof lands by the row's last DAA.
#[test]
fn a_carrier_stuck_in_our_own_mempool_yields_to_the_proof_within_the_escalation_margin() {
    use kaspa_consensus_core::network::{NetworkId, NetworkType};
    use kaspa_consensus_core::palw_model_registry_v1::{PalwSeatReadinessRowV1, palw_readiness_duty_due_v2};
    let t12 = kaspa_consensus_core::config::params::Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &t12.palw_consensus_mode else {
        panic!("testnet-12 is ConsensusV2")
    };
    let g = kaspa_consensus_core::palw_model_registry_v1::palw_registry_globals_of_bundle_v1(bundle);
    let row = PalwSeatReadinessRowV1 { proved_daa: 100, proved_span: 100, leaf_index: 0, proof_version: 2, chunks: 16 };
    let (first_escalated, last_fresh) = (122u64, 124u64);
    let tick = |now: u64, stuck_at: u64, fixed: bool| -> bool {
        let last = Some(100u64);
        let urgency =
            crate::palw_readiness_escalation::palw_readiness_duty_urgency_v1(true, Some(&row), now, 2, last, now, 1, &g, true);
        let due = palw_readiness_duty_due_v2(Some(&row), now, now, last, 1, &g, true)
            && !crate::palw_readiness_escalation::palw_readiness_duty_waits_v1(true, last, now, 1);
        if !fixed {
            return false; // the slot is held: every site finds `inflight` at the cap
        }
        let late = now - stuck_at;
        // The tick's sites, as wired: the escalated one (late), the Own site (lost), and — V01's
        // panel side — any due duty, the proof included, once the carrier is stuck.
        (urgency.is_some() && palw_readiness_replaces_carrier_v1(true, late))
            || ((due || urgency.is_some()) && palw_readiness_replaces_carrier_v1(urgency.is_some(), late))
            || ((due || urgency.is_some()) && palw_stuck_carrier_opens_slot_v1(late))
    };
    let at = first_escalated;
    assert_eq!(
        crate::palw_readiness_escalation::palw_readiness_duty_urgency_v1(true, Some(&row), at, 2, Some(100), at, 1, &g, true),
        Some(kaspa_consensus_core::palw_readiness_escalation_v1::PalwReadinessUrgencyV1::Lapsing { last_fresh_daa: last_fresh }),
        "testnet-12's horizon, as the tick reads it"
    );
    for stuck_at in 100..=last_fresh {
        let old = (stuck_at + 1..=stuck_at + 40).find(|now| tick(*now, stuck_at, false));
        assert_eq!(old, None, "stuck at {stuck_at}: the drill's tick never sends, and the row lapses");
        let sent = (stuck_at + 1..=stuck_at + 40).find(|now| tick(*now, stuck_at, true)).expect("the proof goes out");
        // Lost before the escalation: the Own site's proof replaced it; late at it: the escalated one.
        assert!(sent <= first_escalated.max(stuck_at + PALW_CARRIER_LATE_DAA_V1), "stuck at {stuck_at}: sent at {sent}");
        if stuck_at + PALW_CARRIER_LATE_DAA_V1 <= first_escalated {
            assert!(
                sent + PALW_READINESS_ESCALATION_LANDING_DAA_V1 <= last_fresh,
                "stuck at {stuck_at}: the proof sent at {sent} lands by the row's last DAA {last_fresh}"
            );
        }
    }
    // The drill's case: stuck since 110 and never landing, replaced by the due proof once stuck
    // (V01's panel side; 120, the lost horizon, before it) — eight DAA before the row would even
    // escalate.
    assert_eq!((111..=125).find(|now| tick(*now, 110, true)), Some(110 + PALW_CARRIER_STUCK_DAA_V1));
    // A merely-due proof never cuts ahead of a carrier that is only slow.
    assert!(!palw_readiness_replaces_carrier_v1(false, PALW_CARRIER_LOST_DAA_V1 - 1));
    assert!(palw_readiness_replaces_carrier_v1(false, PALW_CARRIER_LOST_DAA_V1));
    assert!(!palw_readiness_replaces_carrier_v1(true, PALW_CARRIER_LATE_DAA_V1 - 1));
    assert!(palw_readiness_replaces_carrier_v1(true, PALW_CARRIER_LATE_DAA_V1));
    assert_eq!(PALW_CARRIER_LATE_DAA_V1, PALW_READINESS_ESCALATION_LANDING_DAA_V1, "late is M1's landing margin");
}

/// **The tick is wired as the simulation above reads it** (b): the late tip is measured off the
/// chained funding while held; the escalated site replaces it before it would carry, the Own site
/// replaces a lost one; the replacement is the pool's RBF over the late carrier's own input, only
/// for a carrier that moves no money; and a replaced tip writes the chain back.
#[test]
fn the_tick_replaces_our_own_late_carrier_with_the_proof() {
    let source = production();
    let tick =
        &source[source.find_code("let mut held = carrier_chain.inflight() >= MAX_INFLIGHT_CARRIERS;").expect("the held tick")..];
    let stuck =
        tick.find_code("let stuck_tip: Option<(kaspa_consensus_core::tx::TransactionId, u64)> = if held {").expect("the late tip");
    let escalated = tick.find_code("palw_readiness_replaces_carrier_v1(true, *late)").expect("the escalated site's test");
    let replace = tick
        .find_code("self.replace_late_carrier_v1(&session, &duty, stuck, late, current_daa, &mut carrier_chain.book_with_funding(&mut funding))")
        .expect("replace");
    let carry = tick
        .find_code("self.carry_readiness_proof_v1(&session, &duty, current_daa, &mut carrier_chain.book_with_funding(&mut funding))")
        .expect("the carry");
    let own = tick.find_code("palw_readiness_replaces_carrier_v1(duty.escalates(), late)").expect("the Own site's test");
    assert!(
        stuck < escalated && escalated < replace && replace < carry && carry < own,
        "measured, then the escalated site, then the Own site"
    );
    assert!(tick.contains_code("slots.rewind_v1(carrier_chain.inflight());"), "a failed replacement is not the lane of record");
    let tail = &tick[tick.find_code("last_lane = slots.finish(carrier_chain.inflight());").expect("the tail")..];
    assert!(
        tail.contains_code("if !held {\n                    carrier_chain.set_funding(funding);"),
        "a replaced tip writes the chain back"
    );
    let body = &source[source.find_code("    async fn replace_late_carrier_v1(").expect("the replacement fn")..];
    let body = &body[..body.find("\n    }\n").expect("its end")];
    assert!(body.contains_code("palw_carrier_replaceable_v1(&late.tx)"));
    assert!(body.contains_code("session.get_virtual_utxo_entry(input)"), "only a confirmed input");
    assert!(body.contains_code("Some(late_feerate)"), "above the late carrier's feerate");
    assert!(
        body.contains_code("self.submit_prepared_replacement_v1(session, prepared_carrier)"),
        "the pool's RBF, never a double spend"
    );
}

fn carrier(object: PalwConsensusObjectV2, outputs: usize) -> Transaction {
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).unwrap();
    let input = TransactionInput::new(TransactionOutpoint::new(Hash64::from_u64_word(9), 0), vec![], 0, 1);
    let out = TransactionOutput::new(1_000, kaspa_consensus_core::tx::ScriptPublicKey::from_vec(0, vec![0x51]));
    Transaction::new(0, vec![input], vec![out; outputs], 0, SUBNETWORK_ID_PALW_LIFECYCLE.clone(), 0, payload)
}

/// **What a proof may replace, and what it pays** (b): a lane object with one input and one change
/// output — never a carrier that moves money or registers — and strictly above the replaced
/// feerate, at the drill's own figures (m7's `ObjectiveOffence`: 388,312 sompi over 31,065 mass).
#[test]
fn a_proof_replaces_only_a_lane_carrier_and_pays_above_it() {
    let proof = PalwConsensusObjectV2::SeatReadinessProvedV2 {
        bond: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(1), 0)),
        class_id: Hash64::from_u64_word(2),
        span: 3,
        proof: Box::new(kaspa_consensus_core::palw_artifact::PalwArtifactMultiproofV1 {
            leaf_count: 1,
            opened: vec![],
            siblings: vec![],
        }),
        signature: vec![],
    };
    assert!(palw_carrier_replaceable_v1(&carrier(proof.clone(), 1)), "a lane object on one input and its change");
    assert!(!palw_carrier_replaceable_v1(&carrier(proof.clone(), 2)), "a carrier with an output beside its change");
    let bond = PalwConsensusObjectV2::BondRegistered {
        bond: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(1), 0)),
        pubkey: vec![1],
        operator_pubkey: vec![2],
        collateral: 1,
        payout_payload: Hash64::from_u64_word(3),
        capable_classes: Default::default(),
        signature: vec![],
    };
    assert!(!palw_carrier_replaceable_v1(&carrier(bond, 1)), "a registration is never replaced");
    let envelope = PalwConsensusObjectV2::SignedRegistrationV1 {
        registration: Box::new(PalwConsensusObjectV2::ObjectChunk {
            group: Hash64::from_u64_word(12),
            index: 0,
            count: 1,
            bytes: vec![],
        }),
        valid_from_daa: 1,
        valid_until_daa: 32,
        fork_digest: Default::default(),
        signer: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(13), 0)),
        signature: vec![],
    };
    // Even a malformed wrapper must not be swapped for another object by
    // the money-moving replacement path; native inclusion judges its bytes.
    assert!(!palw_carrier_replaceable_v1(&carrier(envelope, 1)), "signed registrations are never replaced");
    let mut garbage = carrier(proof.clone(), 1);
    garbage.payload = vec![0xFF; 3];
    assert!(!palw_carrier_replaceable_v1(&garbage), "an object this node cannot read");
    let mut native = carrier(proof, 1);
    native.subnetwork_id = kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
    assert!(!palw_carrier_replaceable_v1(&native), "not a lifecycle carrier");
    let replaced_feerate = 388_312f64 / 31_065f64;
    for mass in [20_000u64, 31_065, 45_000] {
        let floor = kaspa_pq_validator_core::relay_fee_for_compute_mass(mass);
        let fee = palw_replacement_fee_v1(floor, replaced_feerate, mass);
        assert!(fee >= floor, "never under the relay floor");
        assert!(fee as f64 / mass as f64 > replaced_feerate, "mass {mass}: strictly above the replaced feerate");
    }
    assert_eq!(palw_replacement_fee_v1(500, 0.0, 0), 500, "nothing to beat: the floor");
}

/// **The review of the shipped node (2026-09-25), LOW: a court move or a data-availability answer
/// is replaced only once its own lane re-plans it.** On 0e8ec984e an escalated proof replaced any
/// lane carrier of ours 2 DAA late ([`PALW_CARRIER_LATE_DAA_V1`]), and the court lane re-plans a
/// move only [`COURT_MOVE_REPLAN_DAA`] after it sent it (`court_moved`): for eight DAA the move
/// was in neither the pool nor the queue, and one sent in the last of its 42-DAA turn missed its
/// deadline — the default a missed rung or answer convicts. Now such a carrier yields only once
/// LOST, at the re-plan horizon itself, so at every DAA the escalated site would replace it the
/// lane has already re-queued it; every other lane carrier (the drill's duplicate
/// `ObjectiveOffence`, a proof of our own) yields late, as before.
#[test]
fn a_court_move_is_replaced_only_once_its_lane_re_plans_it() {
    use kaspa_consensus_core::{palw_attn_court_v1, palw_bisect, palw_offence_v1};
    let session_id = Hash64::from_u64_word(7);
    let bond = PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(1), 0));
    let moves = [
        PalwConsensusObjectV2::CourtOpened {
            session_id,
            claim: Hash64::from_u64_word(8),
            challenger_bond: bond,
            space: palw_bisect::PalwBisectSpaceV1::StepLeaves,
            space_size: 8,
            signature: vec![],
        },
        PalwConsensusObjectV2::CourtVerdictPosted {
            session_id,
            verdict: palw_bisect::PalwBisectVerdictV1 { version: 1, session_id, round: 3, agree: true },
            signature: vec![],
        },
        PalwConsensusObjectV2::CourtAttnChildChosen {
            session_id,
            choice: palw_attn_court_v1::PalwAttnDissectChoiceV1 { version: 1, session_id, round: 2, child: 1 },
            signature: vec![],
        },
    ];
    // The court lane's debounce, as the duty loop reads `court_moved`: a move sent at `sent` is
    // skipped while `current_daa < sent + COURT_MOVE_REPLAN_DAA`, and re-planned from then on.
    let replanned = |sent: u64, now: u64| now >= sent.saturating_add(COURT_MOVE_REPLAN_DAA);
    let sent = 1_000u64;
    // The gap the review found: late at 2 DAA, and not re-planned for 8 more.
    assert!(
        palw_readiness_replaces_carrier_v1(true, PALW_CARRIER_LATE_DAA_V1) && !replanned(sent, sent + PALW_CARRIER_LATE_DAA_V1),
        "the shipped escalated site replaced a move its lane had not re-planned"
    );
    for object in moves {
        let name = object_name(&object);
        let tx = carrier(object, 1);
        assert!(palw_carrier_replaceable_v1(&tx), "{name}: a lane object on one input and its change");
        for now in sent..sent + 3 * COURT_MOVE_REPLAN_DAA {
            let late = now - sent;
            // Either site, as wired: the carrier is late (or lost) for the proof, and it yields.
            let replaced = palw_readiness_replaces_carrier_v1(true, late) && !palw_carrier_keeps_its_turn_v1(&tx, late);
            assert_eq!(replaced, late >= PALW_CARRIER_LOST_DAA_V1, "{name}, {late} DAA unconfirmed");
            assert!(!replaced || replanned(sent, now), "{name}: replaced {late} DAA late, before its lane re-plans it");
            // And the stuck opening (V01, rcore/n1-carrier), which would otherwise open the move's
            // slot to any duty at STUCK (4 DAA) — 4..10 DAA before its lane re-plans it.
            let opened = palw_stuck_carrier_opens_slot_v1(late) && !palw_carrier_keeps_its_turn_v1(&tx, late);
            assert_eq!(opened, late >= PALW_CARRIER_LOST_DAA_V1, "{name}, {late} DAA unconfirmed: the stuck opening");
            assert!(!opened || replanned(sent, now), "{name}: its slot opened {late} DAA late, before its lane re-plans it");
        }
    }
    // Every other lane carrier yields late, as before — the drill's own case above all.
    let offence = PalwConsensusObjectV2::ObjectiveOffence {
        kind: palw_offence_v1::PalwOffenceKindV1::ExecutorEquivocation,
        accused: bond,
        evidence_id: Hash64::from_u64_word(9),
        evidence: vec![],
    };
    assert!(!palw_carrier_keeps_its_turn_v1(&carrier(offence, 1), PALW_CARRIER_LATE_DAA_V1), "the drill's ObjectiveOffence");
    let proof = PalwConsensusObjectV2::SeatReadinessProvedV2 {
        bond,
        class_id: Hash64::from_u64_word(2),
        span: 3,
        proof: Box::new(kaspa_consensus_core::palw_artifact::PalwArtifactMultiproofV1 {
            leaf_count: 1,
            opened: vec![],
            siblings: vec![],
        }),
        signature: vec![],
    };
    assert!(!palw_carrier_keeps_its_turn_v1(&carrier(proof, 1), PALW_CARRIER_LATE_DAA_V1), "a proof of our own");
    // Every court move and every data-availability answer is named (the answers carry a whole
    // step binding, so the list is read rather than built).
    let source = production();
    let keeps = &source[source.find("pub(crate) fn palw_carrier_keeps_its_turn_v1(").expect("the rule")..];
    let keeps = &keeps[..keeps.find("\n}\n").expect("its end")];
    for variant in [
        "CourtClosed",
        "CourtDisclosed",
        "CourtCloseDeclared",
        "CourtCloseChunk",
        "CourtAttnRootClaimed",
        "CourtAttnRootClaimedAnchored",
        "CourtAttnRootClaimedHeld",
        "CourtAttnDissected",
        "CheckpointAccused",
        "MaterialDisclosed",
        "MaterialDisclosedHeld",
        "MaterialDisclosedV2",
    ] {
        assert!(keeps.contains(&format!("PalwConsensusObjectV2::{variant} {{ .. }}")), "{variant} keeps its turn");
    }
    // And the replacement asks it before it builds anything.
    let replace = &source[source.find("    async fn replace_late_carrier_v1(").expect("the replacement fn")..];
    let replace = &replace[..replace.find("\n    }\n").expect("its end")];
    let asked = replace.find("palw_carrier_keeps_its_turn_v1(&late.tx, unconfirmed_daa)").expect("asked");
    assert!(asked < replace.find("self.build_lifecycle_tx_priced_v1(").expect("the build"), "before the build");
    // The stuck opening asks it too, before it arms the opening or hands out the input.
    let open = &source[source.find("    async fn open_stuck_carrier_v1(").expect("the opening fn")..];
    let open = &open[..open.find("\n    }\n").expect("its end")];
    let asked = open.find("palw_carrier_keeps_its_turn_v1(&late.tx, unconfirmed_daa)").expect("asked by the opening");
    assert!(asked < open.find("session.get_virtual_utxo_entry(input)?").expect("the input"), "before the input");
    assert!(asked < open.find("*self.carrier_replacement.lock().unwrap() =").expect("the arming"), "before it arms");
    // A move that keeps its turn declines the escalated site's replacement — and the escalated proof
    // it declined still rides the readiness lane (V07, rcore/n1-carrier), which the escalated site
    // offers only when nothing is being replaced: the declined arm offers it the removed duty
    // (the int-4 audit, n1 x n2), so the ordinary proofs never ride the lane ahead of it.
    let tick =
        &source[source.find_code("let mut held = carrier_chain.inflight() >= MAX_INFLIGHT_CARRIERS;").expect("the held tick")..];
    let site = tick
        .find_code("if self.replace_late_carrier_v1(&session, &duty, stuck, late, current_daa, &mut carrier_chain.book_with_funding(&mut funding)).await {")
        .expect("the escalated site's replacement");
    let site = &tick[site..];
    let site = &site[..site
        .find_code("} else if slots.offers(PalwCarrierSiteV1::ReadinessEscalated, carrier_chain.inflight())")
        .expect("the next arm")];
    let declined = &site[site.find("} else {").expect("the declined arm")..];
    assert!(
        declined.contains("!self.carry_on_readiness_lane_v1(&session, &duty, current_daa, &mut readiness_lane, &main).await;"),
        "a declined escalated replacement offers the lane: {declined}"
    );
}

/// A replacement that did not go out leaves the site it entered with nothing sent.
#[test]
fn a_rewound_site_is_not_the_lane_of_record() {
    let mut slots = PalwCarrierSlotsV1::new(Some(PalwCarrierLaneV1::Licence));
    slots.at(PalwCarrierSiteV1::ReadinessEscalated, 0);
    slots.rewind_v1(1);
    assert_eq!(slots.finish(1), Some(PalwCarrierLaneV1::Licence), "the lane before it stands");
    let mut sent = PalwCarrierSlotsV1::new(Some(PalwCarrierLaneV1::Licence));
    sent.at(PalwCarrierSiteV1::ReadinessEscalated, 0);
    assert!(matches!(sent.finish(1), Some(PalwCarrierLaneV1::Readiness { .. })), "a replacement that went out is the proof's slot");
}

/// **V01's panel side (the pre-t12 sweep of 2026-09-25): a stuck carrier of ANY lane yields its slot
/// to the next duty due.** The drill's m7 held its one slot with a duplicate `ObjectiveOffence` for
/// 52 minutes; the readiness fix let only a possession proof take it back, so a stuck court move,
/// licence, receipt or filing still held every other duty. Now every site of the tick gets the
/// slot once the carrier is stuck — twice the landing margin, before any lane's own re-plan — and
/// never before: a carrier that is merely slow keeps it. The opening prices and submits as a
/// replacement exactly the carrier on the stuck input, and nothing once one went out.
#[test]
fn a_stuck_carrier_of_any_lane_yields_its_slot_to_the_next_duty_due() {
    use PalwCarrierSiteV1::*;
    assert_eq!(PALW_CARRIER_STUCK_DAA_V1, 2 * PALW_CARRIER_LATE_DAA_V1, "twice M1's landing margin");
    assert!(PALW_CARRIER_STUCK_DAA_V1 < PALW_CARRIER_LOST_DAA_V1, "before any lane's own re-plan horizon");
    assert!(!palw_stuck_carrier_opens_slot_v1(PALW_CARRIER_STUCK_DAA_V1 - 1), "a slow carrier keeps its slot");
    assert!(palw_stuck_carrier_opens_slot_v1(PALW_CARRIER_STUCK_DAA_V1));
    // One duty due, of each lane, our carrier stuck since DAA 0 and never landing, whatever lane
    // the last carrier rode. The old tick: held at every site for good (only a proof could replace
    // a stuck carrier). The new one: the slot opens at the stuck horizon, the duty's site takes it
    // in the tick's own order, and nothing is chained on the replacement.
    let tick_sends = |last: Option<PalwCarrierLaneV1>, holds: fn(PalwCarrierSiteV1) -> bool| -> Vec<PalwCarrierSiteV1> {
        let mut slots = PalwCarrierSlotsV1::new(last);
        let (mut inflight, mut sent) = (0usize, Vec::new());
        for site in PalwCarrierSiteV1::TICK_ORDER {
            slots.at(site, inflight);
            if slots.offers(site, inflight) && holds(site) {
                inflight += 1;
                sent.push(site);
            }
        }
        sent
    };
    let duties: [(&str, fn(PalwCarrierSiteV1) -> bool); 5] = [
        ("a court move, an accusation, an answer or a filing", |at| matches!(at, PriorityFirst | PriorityAfterLicences)),
        ("a licence or a supplementary set", |at| at == Licences),
        ("a seat's own receipt", |at| at == OwnReceipts),
        ("a due proof or the class registration", |at| at == Own),
        ("an escalated proof", |at| matches!(at, ReadinessEscalated | Own)),
    ];
    let lasts = [
        None,
        Some(PalwCarrierLaneV1::Priority),
        Some(PalwCarrierLaneV1::Licence),
        Some(PalwCarrierLaneV1::Ordinary),
        Some(PalwCarrierLaneV1::Readiness { licence_turn: true }),
        Some(PalwCarrierLaneV1::Readiness { licence_turn: false }),
    ];
    for (duty, holds) in duties {
        for last in lasts {
            let tick = |late: u64, fixed: bool| -> Vec<PalwCarrierSiteV1> {
                if !(fixed && palw_stuck_carrier_opens_slot_v1(late)) {
                    return Vec::new(); // held: every site finds `inflight` at the cap
                }
                tick_sends(last, holds)
            };
            assert!((0..=720).all(|late| tick(late, false).is_empty()), "{duty}: the old tick never sends");
            let sent = (0..=40).find(|late| !tick(*late, true).is_empty());
            assert_eq!(sent, Some(PALW_CARRIER_STUCK_DAA_V1), "{duty} after {last:?}: the due duty takes the stuck slot");
            assert_eq!(tick(PALW_CARRIER_STUCK_DAA_V1, true).len(), 1, "{duty}: the replacement alone, nothing chained on it");
        }
    }
    // The opening: keyed by the stuck carrier's input, closed once one carrier went out.
    let proof = PalwConsensusObjectV2::DefaultAccused {
        claim: Hash64::from_u64_word(0xC1),
        missing_event_index: 0,
        accuser: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(0xB0), 0)),
        signature: vec![1; 8],
    };
    let on_input = carrier(proof.clone(), 1); // spends (9, 0)
    let mut elsewhere = carrier(proof, 1);
    elsewhere.inputs[0].previous_outpoint = TransactionOutpoint::new(Hash64::from_u64_word(10), 0);
    let input = TransactionOutpoint::new(Hash64::from_u64_word(9), 0);
    let mut opening = PalwCarrierReplacementV1 {
        input,
        stuck: kaspa_consensus_core::tx::TransactionId::from_u64_word(0x57),
        feerate: 388_312f64 / 31_065f64,
        unconfirmed_daa: PALW_CARRIER_STUCK_DAA_V1,
        replaced_by: None,
    };
    assert_eq!(opening.floor_for(&input), Some(388_312f64 / 31_065f64), "a carrier on the stuck input pays above it");
    assert_eq!(opening.floor_for(&elsewhere.inputs[0].previous_outpoint), None, "no other carrier is repriced");
    assert!(opening.is_replaced_by(&on_input) && !opening.is_replaced_by(&elsewhere), "only it is sent as the replacement");
    opening.replaced_by = Some(on_input.id());
    assert_eq!(opening.floor_for(&input), None, "closed once one went out");
    assert!(!opening.is_replaced_by(&on_input), "and nothing after it is sent as a replacement");
    // What may be replaced is the readiness fix's rule: a DA accusation is a lane object.
    assert!(palw_carrier_replaceable_v1(&on_input));
}

/// **V07 (the pre-t12 sweep of 2026-09-25): a possession proof the main slot cannot take rides the
/// readiness lane.** The rule: the lane takes a proof exactly when the main slot cannot, one in
/// flight. Its funding reads as the main chain's tip does, and yields to the main chain. And the
/// drill's worst case — the main slot held for good by a carrier the stuck opening may not replace
/// (a class registration), an escalated proof due every few DAA: the old tick never sends one; a
/// seat with a second output sends each at once, one in flight at a time.
#[test]
fn a_proof_the_main_slot_cannot_take_rides_the_readiness_lane() {
    assert!(palw_readiness_lane_takes_v1(false, 0), "the main slot cannot take it: the lane does");
    assert!(!palw_readiness_lane_takes_v1(true, 0), "a proof the main slot takes goes there, as it always did");
    assert!(!palw_readiness_lane_takes_v1(false, PALW_READINESS_LANE_INFLIGHT_V1), "one proof in flight on the lane");
    let entry = UtxoEntry {
        amount: 100_000_000,
        script_public_key: kaspa_consensus_core::tx::ScriptPublicKey::default(),
        block_daa_score: 0,
        is_coinbase: false,
    };
    let at = TransactionOutpoint::new(Hash64::from_u64_word(0x1A), 0);
    let funded = |inflight: usize| PalwReadinessLaneV1 { funding: Some((at, entry.clone())), inflight, unfunded_at: None };
    let settled = |mut lane: PalwReadinessLaneV1, confirmed: bool, spent: bool, kept: bool| {
        lane.settle(confirmed, spent, kept);
        (lane.funding.map(|(outpoint, _)| outpoint), lane.inflight)
    };
    assert_eq!(settled(funded(1), true, false, true), (Some(at), 0), "its proof was mined: the lane is whole again");
    assert_eq!(settled(funded(1), false, false, true), (Some(at), 1), "in flight and kept: it waits");
    assert_eq!(settled(funded(1), false, false, false), (None, 0), "neither mined nor kept: resolved afresh");
    assert_eq!(settled(funded(0), true, true, true), (None, 0), "the main chain spent it: the main chain comes first");
    assert_eq!(settled(PalwReadinessLaneV1 { inflight: 1, ..Default::default() }, false, false, false), (None, 0));

    // The worst case, per DAA: an escalated proof due at 0, 3, 6, …; the main slot never takes one;
    // a lane proof lands two DAA after it is sent (M1's landing margin).
    let run = |second_output: bool| -> Vec<u64> {
        let (mut lane_inflight_until, mut waiting, mut sent) = (None::<u64>, false, Vec::new());
        for daa in 0..30u64 {
            if lane_inflight_until.is_some_and(|until| daa >= until) {
                lane_inflight_until = None;
            }
            waiting |= daa % 3 == 0;
            let lane_inflight = usize::from(lane_inflight_until.is_some());
            if waiting && second_output && palw_readiness_lane_takes_v1(false, lane_inflight) {
                sent.push(daa);
                waiting = false;
                lane_inflight_until = Some(daa + PALW_READINESS_ESCALATION_LANDING_DAA_V1);
            }
        }
        sent
    };
    assert!(run(false).is_empty(), "one output: the proofs wait behind the held slot, as before");
    let sent = run(true);
    assert_eq!(sent, (0..30).step_by(3).collect::<Vec<u64>>(), "a second output: every proof goes out when it is due");
    assert!(sent.windows(2).all(|w| w[1] - w[0] >= PALW_READINESS_ESCALATION_LANDING_DAA_V1), "one in flight at a time");
}

/// **The tick is wired as the test above reads it** (V07): the lane's funding is settled with the
/// main chain before the sites; the escalated site offers it a proof the main slot cannot take; the
/// Own site offers it one before the proof waits, past R-core+ only; its funding excludes whatever
/// the main chain holds and whatever our pool spends, by the main chain's own usability rule; and
/// its change is never remembered as the main chain's.
#[test]
fn the_tick_offers_the_readiness_lane_where_the_main_slot_cannot_take_a_proof() {
    let source = production();
    let body = |signature: &str| -> &'static str {
        let start = source.find(signature).unwrap_or_else(|| panic!("{signature}"));
        let rest = &source[start..];
        &rest[..rest.find("\n    }\n").expect("its end")]
    };
    let settle = source.find_code("readiness_lane.settle(confirmed, spent_in_pool, kept);").expect("the lane is settled");
    let held = source.find_code("let mut held = carrier_chain.inflight() >= MAX_INFLIGHT_CARRIERS;").expect("the held tick");
    assert!(settle < held && held - settle < 1_500, "settled with the main chain, before the sites");
    let tick = &source[held..];
    let main_escalated =
        tick.find_code("if self.carry_readiness_proof_v1(&session, &duty, current_daa, &mut carrier_chain.book_with_funding(&mut funding)).await {").unwrap();
    let lane_escalated = tick
        .find_code("if self.carry_on_readiness_lane_v1(&session, &duties[at], current_daa, &mut readiness_lane, &main).await {")
        .expect("the escalated site's lane");
    let priority = tick.find_code("slots.at(PalwCarrierSiteV1::PriorityFirst, carrier_chain.inflight());").unwrap();
    assert!(main_escalated < lane_escalated && lane_escalated < priority, "after the main slot's offer, before the court");
    // A declined replacement of our late tip is a proof the main slot cannot take either (the int-4
    // audit, n1 x n2): the escalated duty it removed from the tick's list is offered to the lane on
    // the declined arm, and the proof waits only when the lane does not take it.
    let replace_escalated = tick
        .find_code("if self.replace_late_carrier_v1(&session, &duty, stuck, late, current_daa, &mut carrier_chain.book_with_funding(&mut funding)).await {")
        .expect("the escalated site's replacement");
    assert!(replace_escalated < main_escalated, "the escalated site's replacement comes first");
    let declined = &tick[replace_escalated..main_escalated];
    let declined = &declined[declined.find_code("} else {").expect("the declined arm")..];
    let lane_declined = declined
            .find_code("readiness_waiting =\n                            !self.carry_on_readiness_lane_v1(&session, &duty, current_daa, &mut readiness_lane, &main).await;")
            .expect("the declined replacement offers the removed duty to the lane");
    assert!(declined[..lane_declined].contains_code("slots.rewind_v1(carrier_chain.inflight());"), "after the site is rewound");
    assert!(
        declined[..lane_declined].chars().filter(|c| !c.is_whitespace()).collect::<String>().contains(
            "funding.iter().chain(carrier_chain.funding().iter()).chain(receipt_lanes.values().filter_map(|l|l.funding.as_ref())).map(|(outpoint,_)|*outpoint).collect();"
        ),
        "the lane's funding excludes both main and receipt-lane chains"
    );
    assert!(!declined.contains_code("readiness_waiting = true;"), "the proof waits only when the lane refuses it too");
    let lane_own = tick
        .find_code("&& self.carry_on_readiness_lane_v1(&session, &duty, current_daa, &mut readiness_lane, &main).await")
        .expect("the Own site's lane");
    let waits = tick.find_code("a possession proof for class {} waits for a carrier slot").unwrap();
    assert!(priority < lane_own && lane_own < waits, "before the proof waits");
    let gate = tick[..lane_own].rfind("self.consensus_config.params.palw_rcore_plus_active_at(current_daa)").expect("the gate");
    assert!(lane_own - gate < 200, "past R-core+ only");
    let funder = body("    async fn resolve_readiness_lane_funding_v1(");
    assert!(funder.contains_code("!main.contains(o) && !self.flow_context.mining_manager().outpoint_is_spent_in_mempool(o)"));
    assert!(funder.contains_code("palw_fee_funding_usable_v1(outpoint, entry, pov_daa, spend_maturity, self.bond, &locked)"));
    assert!(funder.contains_code("scan.offer(outpoint, entry, usable, is_free);"), "the largest output that pays");
    assert!(!funder.contains_code("persist_fee_outpoint"), "nothing remembered");
    let carry = body("    async fn carry_readiness_proof_on_v1(");
    assert!(carry.contains_code("if !lane { self.persist_fee_outpoint(next);"), "the main chain's memory is its own");
    assert!(body("    async fn carry_on_readiness_lane_v1(").contains_code("lane.unfunded_at == Some(current_daa)"), "one scan a DAA");
}

/// **The tick is wired as the test above reads it** (V01's panel side): the stuck tip is measured
/// while held, and the slot is opened on its input before the escalated site (which then defers to
/// the opening); every carrier site of the tick sends through the one door, which submits the
/// carrier on the stuck input as the pool's RBF; the builder prices that carrier above the stuck
/// one; a claim's commitment and the equivocation evidence never ride it; and the opening closes
/// with the tick — a tick that sent nothing leaves the slot held.
#[test]
fn the_tick_opens_a_stuck_carriers_slot_to_every_site_through_one_door() {
    let source = production();
    let body = |signature: &str| -> &'static str {
        let start = source.find(signature).unwrap_or_else(|| panic!("{signature}"));
        let rest = &source[start..];
        &rest[..rest.find("\n    }\n").expect("its end")]
    };
    let tick =
        &source[source.find_code("let mut held = carrier_chain.inflight() >= MAX_INFLIGHT_CARRIERS;").expect("the held tick")..];
    let stuck = tick.find_code("let stuck_tip: Option<(kaspa_consensus_core::tx::TransactionId, u64)> = if held {").expect("stuck");
    let open = tick.find_code("Some((stuck, late)) if palw_stuck_carrier_opens_slot_v1(late) => {").expect("the opening");
    let escalated = tick.find_code("palw_readiness_replaces_carrier_v1(true, *late)").expect("the escalated site");
    assert!(stuck < open && open < escalated, "measured, opened, then the escalated site");
    let opened = &tick[open..escalated];
    for line in [
        "self.open_stuck_carrier_v1(&session, stuck, late)",
        "funding = Some(opened);",
        "carrier_chain.set_inflight(0);",
        "held = false;",
    ] {
        assert!(opened.contains(line), "the opening: {line}");
    }
    assert!(tick[..escalated].trim_end().ends_with("!stuck_opened\n                        &&"), "the escalated site defers to it");
    let tail_at = tick.find_code("last_lane = slots.finish(carrier_chain.inflight());").expect("the tail");
    let (sites, tail) = tick.split_at(tail_at);
    assert_eq!(
        sites.count_code("self.submit_prepared_carrier_v1(&session, prepared_carrier)"),
        7,
        "the existing main sites and RFC-0018 receipt-lane site all use the shared submission door"
    );
    assert_eq!(sites.count_code("self.submit_prepared_direct_v1("), 1, "the canonical claim alone");
    let canonical = sites.find_code("self.submit_prepared_direct_v1(").unwrap();
    assert!(sites[..canonical].rfind("&& !stuck_opened").is_some_and(|guard| canonical - guard < 2_000), "and it never replaces");
    let priority = body("    async fn carry_priority_v1(");
    assert_eq!(priority.count_code("self.submit_prepared_carrier_v1(session, prepared_carrier)"), 2, "court moves and evidence");
    assert!(!priority.contains_code("flow_context.submit_rpc_transaction("));
    assert!(
        priority.contains_code("Some((funding_outpoint, _)) if self.stuck_carrier_open_on_v1(&funding_outpoint) =>"),
        "evidence waits"
    );
    assert!(
        body("    async fn carry_readiness_proof_on_v1(").contains_code("self.submit_prepared_carrier_v1(session, prepared_carrier)")
    );
    let door = body("    async fn submit_carrier_v1(");
    assert!(
        door.contains_code("opening.is_replaced_by(&tx)") && door.contains_code("submit_rpc_transaction_replacement(session, tx)")
    );
    assert!(door.contains_code("open.replaced_by = Some(txid);"));
    let shared = include_str!("carrier_chain.rs");
    for (name, door) in [
        ("submit_prepared_carrier_v1", "self.submit_carrier_v1(session, tx).await?;"),
        (
            "submit_prepared_direct_v1",
            "self.flow_context.submit_rpc_transaction(session, tx, kaspa_mining::mempool::tx::Orphan::Forbidden).await?;",
        ),
        ("submit_prepared_replacement_v1", "self.flow_context.submit_rpc_transaction_replacement(session, tx).await?;"),
    ] {
        let method = &shared[shared.find_code(&format!("fn {name}(")).expect(name)..];
        let method = &method[..method.find("\n    }").expect("submission method ends")];
        assert!(method.contains_code(door), "the original submission door and error remain: {name}");
        assert!(!method.contains_code("record_change("), "caller flags still precede persistence/bookkeeping: {name}");
    }
    let opener = body("    async fn open_stuck_carrier_v1(");
    assert!(opener.contains_code("palw_carrier_replaceable_v1(&late.tx)"), "only a lane object");
    assert!(opener.contains_code("palw_carrier_keeps_its_turn_v1(&late.tx, unconfirmed_daa)"), "a court move only once lost");
    assert!(opener.contains_code("session.get_virtual_utxo_entry(input)?"), "only a confirmed input");
    assert!(body("    fn build_lifecycle_tx_priced_v1(").contains_code("opening.floor_for(&funding_outpoint)"), "priced above it");
    let close = tail.find_code("let opening = self.carrier_replacement.lock().unwrap().take();").expect("closed with the tick");
    let write_back = tail.find_code("if !held {\n                    carrier_chain.set_funding(funding);").expect("the write-back");
    assert!(close < write_back, "a tick that sent nothing is held again before the chain is written back");
    assert!(
        tail[close..write_back]
            .contains_code("carrier_chain.set_inflight(MAX_INFLIGHT_CARRIERS);\n                        held = true;")
    );
}
