//! **The seat's escalated possession proof** (the 2026-09-25 model-registry review, M1) — the
//! node-policy half of `kaspa_consensus_core::palw_readiness_escalation_v1` that the panel's tick
//! reads: which of this tick's proofs escalates, and which one takes the `ReadinessEscalated` site
//! of the one carrier scheduler (`palw_panel::PalwCarrierSlotsV1`).
//!
//! A proof escalates when its row at the tip is about to lapse and the proof renews it (the one
//! predicate the pool and the tip read ask too), past R-core+ only — and not while this seat's own
//! last proof for the class is still landing: a copy sent then cannot land sooner, so hurrying it
//! would only take the court's slot. The scheduler then carries at most one escalated proof a tick,
//! never two slots running, ahead of the court queue; a proof that does not escalate keeps the Own
//! site exactly as before.

use kaspa_consensus_core::palw_model_registry_v1::{PalwRegistryGlobalsV1, PalwSeatReadinessRowV1};
use kaspa_consensus_core::palw_readiness_escalation_v1::{
    PALW_READINESS_ESCALATION_LANDING_DAA_V1, palw_readiness_proof_escalates_v1,
};
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_hashes::Hash64;

/// One possession proof this tick may carry (`PalwPanelService::readiness_duties`), and whether it
/// escalates ([`palw_readiness_duty_escalates_v1`]).
#[derive(Clone, Debug)]
pub struct PalwReadinessDutyV1 {
    pub object: PalwConsensusObjectV2,
    pub escalates: bool,
}

impl PalwReadinessDutyV1 {
    /// The class the proof is for.
    pub fn class_id(&self) -> Option<Hash64> {
        match &self.object {
            PalwConsensusObjectV2::SeatReadinessProved { class_id, .. }
            | PalwConsensusObjectV2::SeatReadinessProvedV2 { class_id, .. } => Some(*class_id),
            _ => None,
        }
    }
}

/// **Is this seat's last proof for a class still landing?** — submitted in `last_submitted_span`,
/// and `now_daa` inside the [`PALW_READINESS_ESCALATION_LANDING_DAA_V1`] a carrier takes from there.
/// On five-DAA spans this never outlasts the span the duty already skips.
pub fn palw_readiness_proof_in_flight_v1(last_submitted_span: Option<u64>, now_daa: u64, span_daa: u64) -> bool {
    last_submitted_span
        .is_some_and(|span| now_daa < span.saturating_mul(span_daa.max(1)).saturating_add(PALW_READINESS_ESCALATION_LANDING_DAA_V1))
}

/// **Does this span's proof escalate?** `armed` (R-core+ in force at `now_daa`), the proof
/// escalates against the tip's `row` (`palw_readiness_proof_escalates_v1`), and this seat's own last
/// proof for the class is not still landing ([`palw_readiness_proof_in_flight_v1`]).
#[allow(clippy::too_many_arguments)]
pub fn palw_readiness_duty_escalates_v1(
    armed: bool,
    row: Option<&PalwSeatReadinessRowV1>,
    span_now: u64,
    proof_version: u8,
    last_submitted_span: Option<u64>,
    now_daa: u64,
    span_daa: u64,
    g: &PalwRegistryGlobalsV1,
    readiness_v2: bool,
) -> bool {
    armed
        && !palw_readiness_proof_in_flight_v1(last_submitted_span, now_daa, span_daa)
        && palw_readiness_proof_escalates_v1(row, span_now, proof_version, now_daa, span_daa, g, readiness_v2)
}

/// **Which proof takes the escalated site**: the first that escalates — one a tick.
pub fn palw_escalated_readiness_pick_v1(duties: &[PalwReadinessDutyV1]) -> Option<usize> {
    duties.iter().position(|duty| duty.escalates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_panel::{PalwCarrierLaneV1, PalwCarrierSiteV1, PalwCarrierSlotsV1};
    use kaspa_consensus_core::palw_model_registry_v1::{
        PALW_REGISTRY_GLOBALS_V1, palw_readiness_duty_due_v2, palw_readiness_max_age_daa_v1,
    };

    const G: PalwRegistryGlobalsV1 = PALW_REGISTRY_GLOBALS_V1;

    fn row(proved_daa: u64) -> PalwSeatReadinessRowV1 {
        PalwSeatReadinessRowV1 { proved_daa, proved_span: proved_daa, leaf_index: 0, proof_version: 2, chunks: 16 }
    }

    /// **Armed only past R-core+, only for a lapsing row, never for a copy of a proof still
    /// landing** — and the pick is the first escalating proof, one a tick.
    #[test]
    fn a_duty_escalates_only_past_the_fence_for_a_lapsing_row_not_already_landing() {
        let r = row(100);
        assert!(palw_readiness_duty_escalates_v1(true, Some(&r), 106, 2, Some(100), 106, 1, &G, true), "age 6: escalated");
        assert!(!palw_readiness_duty_escalates_v1(false, Some(&r), 106, 2, Some(100), 106, 1, &G, true), "the twin: disarmed, never");
        assert!(!palw_readiness_duty_escalates_v1(true, Some(&r), 105, 2, Some(100), 105, 1, &G, true), "age 5: the Own site");
        assert!(!palw_readiness_duty_escalates_v1(true, Some(&r), 107, 2, Some(106), 107, 1, &G, true), "its proof of 106 is landing");
        assert!(palw_readiness_duty_escalates_v1(true, Some(&r), 108, 2, Some(106), 108, 1, &G, true), "…and did not land by 108");
        assert!(!palw_readiness_proof_in_flight_v1(Some(10), 52, 5), "five-DAA spans: the next span is past the landing");
        let object = |class: u64| PalwConsensusObjectV2::SeatReadinessProvedV2 {
            bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
                Hash64::from_u64_word(1),
                0,
            )),
            class_id: Hash64::from_u64_word(class),
            span: 1,
            proof: Box::new(kaspa_consensus_core::palw_artifact::PalwArtifactMultiproofV1 {
                leaf_count: 1,
                opened: vec![],
                siblings: vec![],
            }),
            signature: vec![],
        };
        let duties = vec![
            PalwReadinessDutyV1 { object: object(1), escalates: false },
            PalwReadinessDutyV1 { object: object(2), escalates: true },
            PalwReadinessDutyV1 { object: object(3), escalates: true },
        ];
        assert_eq!(palw_escalated_readiness_pick_v1(&duties), Some(1));
        assert_eq!(duties[1].class_id(), Some(Hash64::from_u64_word(2)));
        assert_eq!(palw_escalated_readiness_pick_v1(&duties[..1]), None, "nothing escalates: nothing is picked");
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Sent {
        Court,
        Licence,
        Proof { class: usize, span: u64 },
    }

    struct Run {
        sent: Vec<(u64, Sent)>,
        lanes: Vec<Option<PalwCarrierLaneV1>>,
        /// `(daa, class)` for every DAA a row counted for nothing.
        lapsed: Vec<(u64, usize)>,
    }

    /// **One seat's tick, one DAA at a time, on testnet-12's clock** (one block a DAA, one-DAA spans,
    /// eight-DAA rows): the tick's own scheduler and site order, the seat's own duty rule and the
    /// escalation, and a carrier's life — sent at `t`, carried by block `t + 1` (which frees the
    /// slot), accepted by block `t + 2` (which writes the row, dated at the span the proof names).
    /// The registry counts the rows a block's parent left, before the block's own objects.
    ///
    /// `proved[c]` is class `c`'s row at DAA 0 (so `vec![0; 2]` is two classes due in the same DAA).
    fn run(ticks: u64, proved: &[u64], armed: bool, court_files: impl Fn(u64) -> bool, licence_waits: impl Fn(u64) -> bool) -> Run {
        let max_age = palw_readiness_max_age_daa_v1(1, &G, true);
        let classes = proved.len();
        let mut rows: Vec<PalwSeatReadinessRowV1> = proved.iter().map(|p| row(*p)).collect();
        let mut landing: Vec<(u64, usize, u64)> = Vec::new();
        let mut last_submitted: Vec<Option<u64>> = vec![None; classes];
        let (mut court, mut last_lane) = (0u64, None);
        let mut out = Run { sent: Vec::new(), lanes: Vec::new(), lapsed: Vec::new() };
        for t in 0..ticks {
            for (class, r) in rows.iter().enumerate() {
                if t.saturating_sub(r.proved_daa) > max_age {
                    out.lapsed.push((t, class));
                }
            }
            landing.retain(|(at, class, span)| {
                if *at == t {
                    rows[*class] = row(*span);
                }
                *at != t
            });
            if court_files(t) {
                court += 1;
            }
            let mut duties: Vec<(usize, bool)> = (0..classes)
                .filter(|c| palw_readiness_duty_due_v2(Some(&rows[*c]), t, t, last_submitted[*c], 1, &G, true))
                .map(|c| (c, palw_readiness_duty_escalates_v1(armed, Some(&rows[c]), t, 2, last_submitted[c], t, 1, &G, true)))
                .collect();
            let mut slots = PalwCarrierSlotsV1::new(last_lane);
            let mut inflight = 0usize; // last tick's carrier was carried by this block
            for site in PalwCarrierSiteV1::TICK_ORDER {
                slots.at(site, inflight);
                if !slots.offers(site, inflight) {
                    continue;
                }
                let sent = match site {
                    PalwCarrierSiteV1::ReadinessEscalated => duties
                        .iter()
                        .position(|(_, escalates)| *escalates)
                        .map(|at| Sent::Proof { class: duties.remove(at).0, span: t }),
                    PalwCarrierSiteV1::PriorityFirst | PalwCarrierSiteV1::PriorityAfterLicences => (court > 0).then(|| {
                        court -= 1;
                        Sent::Court
                    }),
                    PalwCarrierSiteV1::Own => (!duties.is_empty()).then(|| Sent::Proof { class: duties.remove(0).0, span: t }),
                    PalwCarrierSiteV1::Licences => licence_waits(t).then_some(Sent::Licence),
                    PalwCarrierSiteV1::OwnReceipts => None,
                };
                if let Some(sent) = sent {
                    inflight += 1;
                    if let Sent::Proof { class, span } = sent {
                        last_submitted[class] = Some(span);
                        landing.push((t + 2, class, span));
                    }
                    out.sent.push((t, sent));
                }
            }
            last_lane = slots.finish(inflight);
            out.lanes.push(last_lane);
        }
        out
    }

    /// **The storm: a court queue that never empties and a licence always waiting.** Today's order
    /// carries a seat's proofs only on the licences' turn, so two classes due in the same DAA put the
    /// second one's carrier in at age 7 and its row lapses; escalated, the second goes at age 6
    /// ahead of the court and no row ever lapses — while the court still gets most slots, never
    /// loses two running to an escalation, and the licences are still carried.
    #[test]
    fn a_da_storm_never_lapses_a_row_and_the_court_still_flows() {
        const TICKS: u64 = 1_000;
        for classes in [1usize, 2] {
            let armed = run(TICKS, &vec![0; classes], true, |_| true, |_| true);
            assert!(
                armed.lapsed.is_empty(),
                "{classes} classes: no row lapses under the storm: {:?}",
                &armed.lapsed[..armed.lapsed.len().min(5)]
            );
            let court = armed.sent.iter().filter(|(_, s)| *s == Sent::Court).count();
            assert!(court * 5 >= TICKS as usize * 2, "{classes} classes: the court keeps two slots in five ({court} of {TICKS})");
            assert!(armed.sent.iter().any(|(_, s)| *s == Sent::Licence), "licences are still carried");
            assert!(
                armed.lanes.windows(2).all(|pair| pair != [Some(PalwCarrierLaneV1::Readiness), Some(PalwCarrierLaneV1::Readiness)]),
                "an escalation never takes two slots running"
            );
            let proofs = armed.sent.iter().filter(|(_, s)| matches!(s, Sent::Proof { .. })).count();
            let escalated = armed.lanes.iter().filter(|lane| **lane == Some(PalwCarrierLaneV1::Readiness)).count();
            println!("storm, {classes} classes: court {court}, proofs {proofs} ({escalated} escalated) of {TICKS} slots");
        }
        let today = run(TICKS, &[0, 0], false, |_| true, |_| true);
        assert!(!today.lapsed.is_empty(), "the hole this closes: today's order lapses a row under the same storm");
    }

    /// **Sparse traffic: nothing changes.** A court move every tenth DAA, no licences, the seat's
    /// classes not all due in one DAA: its proofs go at the Own site at age 5 as they always did, and
    /// one a court move held to age 6 goes the next DAA either way (escalated, it rides the new site
    /// — the same carrier in the same DAA). The same carriers, the same DAA, no row lapsing, armed or
    /// not. (Two classes due in the SAME DAA are not sparse for a one-carrier seat: the second reaches
    /// age 6 behind the first proof's own re-send, and there the escalation sends it ahead of that
    /// duplicate — the storm test's case.)
    #[test]
    fn sparse_traffic_sends_the_same_carriers_armed_or_not() {
        for proved in [vec![0u64], vec![0, 3], vec![0, 2, 4]] {
            let armed = run(500, &proved, true, |t| t % 10 == 3, |_| false);
            let today = run(500, &proved, false, |t| t % 10 == 3, |_| false);
            assert_eq!(armed.sent, today.sent, "{proved:?}: the same carriers, in the same DAA");
            assert!(armed.lapsed.is_empty() && today.lapsed.is_empty(), "{proved:?}: no row lapses");
        }
    }

    // ---- M1's measurement: no drill, the real prover path on the in-tree A16 fixture ----

    /// The carrier `PalwPanelService::build_lifecycle_tx` builds, byte for byte in shape: one
    /// ML-DSA-87 P2PKH input signed over its sighash, one change output to the same script, the
    /// object in a 0x4b payload, the relay minimum fee for its compute mass.
    fn carrier_tx(
        object: &PalwConsensusObjectV2,
        params: &kaspa_consensus_core::config::params::Params,
        kp: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
    ) -> (kaspa_consensus_core::tx::Transaction, kaspa_consensus_core::tx::UtxoEntry) {
        use kaspa_consensus_core::constants::{MAX_TX_IN_SEQUENCE_NUM, SOMPI_PER_KASPA, TX_VERSION};
        use kaspa_consensus_core::hashing::sighash::{Mldsa87SigHashReusedValuesUnsync, calc_mldsa87_signature_hash};
        use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
        use kaspa_consensus_core::mass::MassCalculator;
        use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
        use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE;
        use kaspa_consensus_core::tx::{
            MutableTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
        };
        use kaspa_txscript::script_builder::ScriptBuilder;
        let payload =
            borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() }).unwrap();
        let spk = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(
            &kaspa_hashes::blake2b_512_address_payload(kp.verification_key.as_ref()).as_bytes(),
        );
        let funding = UtxoEntry::new(1_000 * SOMPI_PER_KASPA, spk.clone(), 0, false);
        let outpoint = TransactionOutpoint::new(Hash64::from_u64_word(0xF00D), 0);
        let build = |fee: u64, signature_script: Vec<u8>| {
            let mut input = TransactionInput::new(outpoint, vec![], MAX_TX_IN_SEQUENCE_NUM, 1);
            input.signature_script = signature_script;
            Transaction::new(
                TX_VERSION,
                vec![input],
                vec![TransactionOutput::new(funding.amount - fee, spk.clone())],
                0,
                SUBNETWORK_ID_PALW_LIFECYCLE,
                0,
                payload.clone(),
            )
        };
        let masses = MassCalculator::new(
            params.mass_per_tx_byte,
            params.mass_per_script_pub_key_byte,
            params.mass_per_sig_op,
            params.storage_mass_parameter,
        );
        let dummy = ScriptBuilder::new()
            .add_data(&vec![0u8; kaspa_txscript::MLDSA87_SIG_LEN + 1])
            .and_then(|b| b.add_data(kp.verification_key.as_ref()))
            .map(|b| b.drain())
            .unwrap();
        let fee =
            kaspa_pq_validator_core::relay_fee_for_compute_mass(masses.calc_non_contextual_masses(&build(1, dummy)).compute_mass);
        let mtx = MutableTransaction::with_entries(build(fee, vec![]), vec![funding.clone()]);
        let sighash = calc_mldsa87_signature_hash(&mtx.as_verifiable(), 0, SIG_HASH_ALL, &Mldsa87SigHashReusedValuesUnsync::new());
        let mut sig = libcrux_ml_dsa::ml_dsa_87::sign(
            &kp.signing_key,
            sighash.as_bytes().as_slice(),
            kaspa_txscript::MLDSA87_TX_CONTEXT,
            [0u8; 32],
        )
        .unwrap()
        .as_ref()
        .to_vec();
        sig.push(SIG_HASH_ALL.to_u8());
        let mut tx = mtx.tx;
        tx.inputs[0].signature_script =
            ScriptBuilder::new().add_data(&sig).and_then(|b| b.add_data(kp.verification_key.as_ref())).map(|b| b.drain()).unwrap();
        tx.finalize();
        (tx, funding)
    }

    struct Weighed {
        object_bytes: usize,
        operand_bytes: usize,
        opened: usize,
        siblings: usize,
        tx_bytes: u64,
        compute_mass: u64,
        transient_mass: u64,
        storage_mass: u64,
    }

    fn weigh(
        object: &PalwConsensusObjectV2,
        params: &kaspa_consensus_core::config::params::Params,
        kp: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
    ) -> Weighed {
        use kaspa_consensus_core::mass::{MassCalculator, transaction_estimated_serialized_size};
        let PalwConsensusObjectV2::SeatReadinessProvedV2 { proof, .. } = object else { panic!("a V2 proof") };
        let (tx, funding) = carrier_tx(object, params, kp);
        let masses = MassCalculator::new(
            params.mass_per_tx_byte,
            params.mass_per_script_pub_key_byte,
            params.mass_per_sig_op,
            params.storage_mass_parameter,
        );
        let non_contextual = masses.calc_non_contextual_masses(&tx);
        let storage = masses
            .calc_contextual_masses(
                &kaspa_consensus_core::tx::MutableTransaction::with_entries(tx.clone(), vec![funding]).as_verifiable(),
            )
            .map_or(0, |m| m.storage_mass);
        Weighed {
            object_bytes: borsh::to_vec(object).unwrap().len(),
            operand_bytes: proof.operand_bytes(),
            opened: proof.opened.len(),
            siblings: proof.siblings.len(),
            tx_bytes: transaction_estimated_serialized_size(&tx),
            compute_mass: non_contextual.compute_mass,
            transient_mass: non_contextual.transient_mass,
            storage_mass: storage,
        }
    }

    /// **M1's input for the consensus-side decision, measured, not assumed** (the 2026-09-25
    /// model-registry review): a real V2 possession proof built by the seat's own prover path on the
    /// in-tree A16 fixture (the drill's `PALW-QWEN25-A16-V5` geometry) — the challenge's draw, the
    /// budget-bounded prefix, the multiproof, both ML-DSA-87 signatures — in the carrier the panel
    /// builds, weighed by the mass calculator under testnet-12's parameters; then the same proof shape
    /// at the shipped class's scale (an inventory of `N` leaves, the shipped A16 row sizes), and from
    /// the consensus constants how many `(bond, class)` rows one block a DAA keeps fresh.
    ///
    /// Run with `--nocapture` to read the table. The asserts pin the arithmetic the table rests on.
    #[test]
    fn the_readiness_proof_is_weighed_against_the_block() {
        use kaspa_consensus_core::config::params::Params;
        use kaspa_consensus_core::network::{NetworkId, NetworkType};
        use kaspa_consensus_core::palw_artifact::{
            PalwArtifactOperandV1, artifact_leaf_v1, palw_artifact_multiproof_v1, verify_artifact_multiproof_v1,
        };
        use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
        use kaspa_consensus_core::palw_model_registry_v1::{
            PALW_READINESS_V2_BUDGET_BYTES_V1, PALW_READINESS_V2_FRAME_BYTES_V1, PALW_READINESS_V2_LEAF_MAX_BYTES_V1,
            PALW_SEAT_READINESS_V2_MLDSA87_CONTEXT, palw_readiness_v2_challenge_seed_v1, palw_readiness_v2_draw_v1,
            palw_readiness_v2_opening_is_the_challenge_v1, palw_seat_readiness_message_v2,
        };
        use kaspa_consensus_core::palw_state_v2::{PALW_OBJECT_CHUNK_MAX_BYTES, PalwBondKeyV2};
        use kaspa_consensus_core::tx::TransactionOutpoint;

        let params = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
        let span_daa = params.palw_execution_lane.as_ref().expect("testnet-12 schedules the lane").schedule_span_daa;
        assert!(params.palw_readiness_v2_at(0) && params.palw_rcore_plus_active_at(0), "testnet-12 arms both from genesis");
        let max_age = palw_readiness_max_age_daa_v1(span_daa, &G, true);
        let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x5E; 32]);
        let bond = PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(0xB0), 0));
        let bond_bytes = borsh::to_vec(&bond).unwrap();
        let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            params.net.to_string().as_bytes(),
            Some(params.genesis.hash),
        );
        let span = 11u64;

        // 1. The in-tree A16 fixture, through the seat's own prover path (`readiness_duties`).
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
        let (root, leaf_count) = backend.artifact_root_and_leaf_count().unwrap();
        let seed = palw_readiness_v2_challenge_seed_v1(&class_id, &bond_bytes, span);
        let draw = palw_readiness_v2_draw_v1(&seed, leaf_count);
        let (_, leaves, drawn) = backend.artifact_readiness_material(&draw).unwrap();
        let (mut opened, mut bytes) = (Vec::new(), 0usize);
        for (index, operand) in drawn.clone() {
            if bytes >= PALW_READINESS_V2_BUDGET_BYTES_V1 {
                break;
            }
            assert!(operand.bytes.len() <= PALW_READINESS_V2_LEAF_MAX_BYTES_V1);
            bytes += operand.bytes.len();
            opened.push((index, operand));
        }
        palw_readiness_v2_opening_is_the_challenge_v1(&draw, &opened.iter().map(|(i, o)| (*i, o.bytes.len())).collect::<Vec<_>>())
            .unwrap();
        opened.sort_by_key(|(index, _)| *index);
        let signed = |class_id: Hash64, proof: kaspa_consensus_core::palw_artifact::PalwArtifactMultiproofV1| {
            let message = palw_seat_readiness_message_v2(domain, &bond_bytes, &class_id, span, &proof);
            let signature = libcrux_ml_dsa::ml_dsa_87::sign(
                &kp.signing_key,
                message.as_byte_slice(),
                PALW_SEAT_READINESS_V2_MLDSA87_CONTEXT,
                [0u8; 32],
            )
            .unwrap()
            .as_ref()
            .to_vec();
            PalwConsensusObjectV2::SeatReadinessProvedV2 { bond, class_id, span, proof: Box::new(proof), signature }
        };
        let proof = palw_artifact_multiproof_v1(&leaves, &opened).expect("the seat holds the artifact");
        verify_artifact_multiproof_v1(&proof, root).expect("the proof opens the registered root");
        let fixture = weigh(&signed(class_id, proof), &params, &kp);
        let overhead = fixture.tx_bytes as usize - fixture.object_bytes;
        println!(
            "testnet-12: span {span_daa} DAA, row age {max_age} DAA, block mass {}, target {} ms a block",
            params.max_block_mass,
            params.target_time_per_block()
        );
        println!(
            "A16 fixture ({leaf_count} leaves): {} of {} leaves opened, {} operand bytes, {} siblings; object {} B (frame {} B); \
             carrier tx {} B (overhead {} B); compute mass {}, transient mass {}, storage mass {}",
            fixture.opened,
            draw.len(),
            fixture.operand_bytes,
            fixture.siblings,
            fixture.object_bytes,
            fixture.object_bytes - fixture.operand_bytes,
            fixture.tx_bytes,
            overhead,
            fixture.compute_mass,
            fixture.transient_mass,
            fixture.storage_mass
        );
        assert_eq!(fixture.transient_mass, fixture.tx_bytes * kaspa_consensus_core::constants::TRANSIENT_BYTE_TO_MASS_FACTOR);

        // 2. The same proof at the shipped class's scale: an inventory of `n` leaves, the drawn leaves
        //    at the shipped A16 row sizes (mean 4,204 B: the prefix stops at the 7th; the worst
        //    case: the budget less one byte, then the largest row, 35,840 B).
        let names: Vec<_> = drawn.iter().map(|(_, o)| (o.tensor_name.clone(), o.layer)).collect();
        let shaped = |n: u32, sizes: &[usize]| {
            let draw = palw_readiness_v2_draw_v1(&seed, n);
            let mut leaves: Vec<Hash64> = (0..n as u64).map(Hash64::from_u64_word).collect();
            let mut opened = Vec::new();
            for (k, (index, size)) in draw.iter().zip(sizes).enumerate() {
                let (tensor_name, layer) = names[k % names.len()].clone();
                let operand = PalwArtifactOperandV1 { tensor_name, layer, row_start: *index, bytes: vec![k as u8; *size] };
                leaves[*index as usize] = artifact_leaf_v1(&operand);
                opened.push((*index, operand));
            }
            palw_readiness_v2_opening_is_the_challenge_v1(&draw, &opened.iter().map(|(i, o)| (*i, o.bytes.len())).collect::<Vec<_>>())
                .expect("the prefix the prover would open");
            opened.sort_by_key(|(index, _)| *index);
            let proof = palw_artifact_multiproof_v1(&leaves, &opened).unwrap();
            weigh(&signed(class_id, proof), &params, &kp)
        };
        let small = vec![1_536usize; 16]; // p50 rows: all sixteen open, under the budget
        let typical = vec![4_204usize; 7];
        let worst = vec![PALW_READINESS_V2_BUDGET_BYTES_V1 - 1, 35_840];
        let mut at_shipped_scale = None;
        for log in [16u32, 18, 19, 20] {
            let n = 1u32 << log;
            let (sm, t, w) = (shaped(n, &small), shaped(n, &typical), shaped(n, &worst));
            println!(
                "A16-shaped, 2^{log} leaves: p50 rows {} B operands ({} opened) -> object {} B (frame {} B), tx {} B, transient \
                 mass {}; typical {} B operands -> object {} B (frame {} B), tx {} B, transient mass {}; \
                 worst {} B operands -> tx {} B, transient mass {}",
                sm.operand_bytes,
                sm.opened,
                sm.object_bytes,
                sm.object_bytes - sm.operand_bytes,
                sm.tx_bytes,
                sm.transient_mass,
                t.operand_bytes,
                t.object_bytes,
                t.object_bytes - t.operand_bytes,
                t.tx_bytes,
                t.transient_mass,
                w.operand_bytes,
                w.tx_bytes,
                w.transient_mass
            );
            assert!(w.object_bytes <= PALW_OBJECT_CHUNK_MAX_BYTES, "the worst proof still rides one carrier");
            assert!(t.object_bytes - t.operand_bytes <= PALW_READINESS_V2_FRAME_BYTES_V1, "the frame is inside its allowance");
            if log == 19 {
                at_shipped_scale = Some((sm, t, w));
            }
        }
        let (small, typical, worst) = at_shipped_scale.unwrap();

        // 3. Rows one block a DAA keeps fresh. A row stands `max_age`; the seat re-proves it at half
        //    that (every `max_age / 2 + 1` DAA), and at the latest it must send by `max_age − landing`.
        let block = params.max_block_mass;
        let lane = block / 2; // P2-9's carrier lane: half a block
        let per_block = |w: &Weighed| block / w.transient_mass.max(w.compute_mass);
        let (cadence, latest) = (max_age / 2 + 1, max_age - PALW_READINESS_ESCALATION_LANDING_DAA_V1);
        let (bonds, classes) = (
            kaspa_consensus_core::config::params::PALW_T12_GENESIS_BONDS.len(),
            kaspa_consensus_core::config::params::PALW_T12_GENESIS_HELD_ROWS.len(),
        );
        println!(
            "testnet-12 demand: {bonds} genesis bonds x {classes} held classes = {} rows; a class needs {} fresh rows to stay out of \
             HELD and {} to leave it — {} and {} rows across the classes",
            bonds * classes,
            G.seat_count,
            G.seat_count + G.spare_seats,
            G.seat_count as usize * classes,
            (G.seat_count + G.spare_seats) as usize * classes
        );
        for (name, w) in [("fixture", &fixture), ("A16 p50 rows", &small), ("A16 typical", &typical), ("A16 worst", &worst)] {
            let p = per_block(w);
            let head = u64::from(w.transient_mass <= lane);
            let max_proof_tx = PALW_OBJECT_CHUNK_MAX_BYTES as u64 + overhead as u64;
            println!(
                "{name}: {p} proofs a block alone -> {} rows kept fresh at the seat's cadence ({cadence} DAA), {} at the latest \
                 ({latest} DAA); under a DA storm with better-paying traffic: {head} a block (the lane's head) -> {} rows; the \
                 largest proof one carrier may hold ({max_proof_tx} B tx) is {} transient mass",
                p * cadence,
                p * latest,
                head * latest,
                max_proof_tx * kaspa_consensus_core::constants::TRANSIENT_BYTE_TO_MASS_FACTOR
            );
        }
        // What the storm keeps of the lane while a proof leads it: an ML-DSA-87 accusation's carrier.
        let accusation = PalwConsensusObjectV2::DefaultAccused {
            claim: Hash64::from_u64_word(0xDA),
            missing_event_index: 0,
            accuser: bond,
            signature: vec![0; kaspa_txscript::MLDSA87_SIG_LEN],
        };
        let accusation_mass =
            kaspa_consensus_core::mass::transaction_estimated_serialized_size(&carrier_tx(&accusation, &params, &kp).0)
                * kaspa_consensus_core::constants::TRANSIENT_BYTE_TO_MASS_FACTOR;
        for (name, w) in [("A16 p50 rows", &small), ("A16 typical", &typical)] {
            let left = lane - w.transient_mass;
            println!(
                "{name} at the lane's head leaves {left} of the lane's {lane}: {} DA accusation carriers of {accusation_mass} mass \
                 (the lane alone holds {})",
                left / accusation_mass,
                lane / accusation_mass
            );
        }
        assert_eq!((cadence, latest), (5, 6), "testnet-12's clock");
        assert_eq!(per_block(&typical), 2, "two typical A16 proofs a block, and no third");
        assert!(typical.transient_mass <= lane, "a typical A16 proof fits the lane's head");
        assert!(worst.transient_mass > lane, "a worst-span A16 proof does not: it rides the fee market even escalated");
    }
}
