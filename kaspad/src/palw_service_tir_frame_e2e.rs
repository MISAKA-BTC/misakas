//! **T-N1 at node level (G14 acceptance test A for the TIR route)**: an IR class onboarded through the PQ mempool and the production
//! template on testnet-12's own ruleset — registered, its family certified, its lane certified, its layout published, its admission
//! jury seated on the seats' readiness proofs and its probation passed by IR attempts going `Final`, to `Active` — then free-prompt
//! claims on it carried as ORIGINAL TIR frames (`TFP1`).
//!
//! Harness exceptions (the parent's): test keys and premine, skipped PoW, inert EVM, and the dormant fences this route needs
//! (`palw_tir_v1`, `palw_staged_public_court_v1`, `palw_staged_tir_route_v1`, `palw_tir_public_conviction_v1` with the TEST-ONLY frame
//! cost — NOT a measurement — and the parent's `palw_legacy_public_filer_v1`) armed in the fixture while the shipping activation
//! validator still refuses them (asserted). Nothing else of the class's path is substituted: every object rides a real carrier.
use super::*;
use kaspa_consensus_core::palw_model_registry_v1::PalwModelLifecycleV1;
use std::path::PathBuf;

// ---- the network ---------------------------------------------------------------------------------------------------------------

/// The parent's testnet-12 fixture with the IR class fence and the TIR frame route armed at genesis (unvalidated: still refused).
pub(super) fn tir_frame_config() -> (Config, PalwConsensusParamsV2, Vec<Funding>, Vec<Funding>) {
    let (config, _, premine, floats) = fixture_config();
    let mut p = config.params.clone();
    let at = Some(ForkActivation::new(0));
    p.palw_tir_v1 = Some(kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1::testnet12_v1(ForkActivation::new(0)));
    p.sync_palw_tir_v1();
    p.palw_staged_public_court_v1 = at;
    p.palw_staged_tir_route_v1 = at;
    p.palw_tir_public_conviction_v1 = at;
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { unreachable!() };
    // The TEST-ONLY frame cost (small windows; not a measurement), its longest door at this network's own liability horizon.
    let mut cost = kaspa_consensus_core::palw_tir_frame_cost_v1::palw_tir_frame_test_only_cost_v1();
    cost.clock.maximum_door_daa = bundle.state.window_challenge_at(0) + bundle.state.window_court();
    p.palw_tir_frame_cost_v1 = Some(cost);
    p.sync_palw_g14_gates_v1();
    assert!(p.validate_palw_v2().is_err(), "the shipping activation guard still refuses the frame route");
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { unreachable!() };
    let bundle = bundle.clone();
    (Config::new(p), bundle, premine, floats)
}

/// Several cards' objects in ONE carrying block (each card once), then the folding block — through the node's mempool check.
pub(super) async fn send_batch(chain: &mut Chain, floats: &mut [Funding], items: Vec<(usize, PalwConsensusObjectV2)>) {
    // Three carriers a block (a readiness proof is ~150 KB of transient mass against the 500,000 block limit).
    for chunk in items.chunks(3) {
        send_block(chain, floats, chunk).await;
    }
    chain.heartbeat(vec![]).await;
}

async fn send_block(chain: &mut Chain, floats: &mut [Funding], items: &[(usize, PalwConsensusObjectV2)]) {
    let daa = chain.daa();
    let mut txs = Vec::new();
    for (card, object) in items {
        let funding = &mut floats[*card];
        let tx = palw_carrier_fees::build_lifecycle_carrier_v2(&chain.config, key(*card), object, funding.0, &funding.1, &[], None, daa)
            .unwrap();
        let mut validated = MutableTransaction::from_tx(tx);
        chain
            .tc
            .validate_mempool_transaction(&mut validated, &Default::default())
            .unwrap_or_else(|e| panic!("the mempool takes card {card}'s carrier: {e}"));
        let tx = validated.tx.as_ref().clone();
        let index = tx.outputs.len() - 1;
        let output = &tx.outputs[index];
        *funding = (
            TransactionOutpoint::new(tx.id(), index as u32),
            UtxoEntry::new(output.value, output.script_public_key.clone(), daa + 1, false),
        );
        txs.push(tx);
    }
    chain.heartbeat(txs).await;
}

// ---- the class -----------------------------------------------------------------------------------------------------------------

fn unhex(s: &str) -> Vec<u8> {
    let mut out = vec![0u8; s.len() / 2];
    faster_hex::hex_decode(s.as_bytes(), &mut out).expect("hex");
    out
}

/// **The IR class**: the golden `dense-gqa-2layer` program under the tiled logits scheme, 8-lane commit tiles (the logits node at
/// 4,096 lanes), two-position checkpoints, four-row history tiles — written as a `PALWTIR1` container (the file a node is deployed
/// with) and loaded through the SDK's door.
pub(super) struct TirFixture {
    _dir: tempfile::TempDir,
    pub path: PathBuf,
    pub entry: misaka_palw_sdk::lineage::PalwTirClassEntryV1,
    pub class_id: Hash64,
    pub root: Hash64,
    pub layout: kaspa_consensus_core::palw_tir_class_v1::PalwTirLayoutV1,
}

impl TirFixture {
    pub fn new(chain: &Chain) -> Self {
        use kaspa_consensus_core::palw_step_refute::{PALW_LOGITS_TILE_LANES, tiled_logits_scheme_id_v1};
        use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
        use misaka_palw_tir::TirProgramV1;
        let vector = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../consensus-vectors/tir-v1/programs/dense-gqa-2layer.json");
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&vector).expect("the golden program")).expect("json");
        let mut program = TirProgramV1::decode_canonical(&unhex(v["program_borsh_hex"].as_str().unwrap())).expect("canonical");
        program.logits_scheme_id.copy_from_slice(tiled_logits_scheme_id_v1().as_byte_slice());
        let program = TirProgramV1::decode_canonical(&program.encode()).expect("still canonical");
        let mut tensors = std::collections::BTreeMap::new();
        for t in v["params"].as_array().unwrap() {
            tensors.insert(
                (t["param"].as_u64().unwrap() as u16, t["layer"].as_u64().map(|l| l as u16)),
                unhex(t["le_hex"].as_str().unwrap()),
            );
        }
        let mut commit_tiles = Vec::new();
        for (bi, b) in program.blocks.iter().enumerate() {
            for (ni, n) in b.nodes.iter().enumerate() {
                if n.commit {
                    let logits = bi == program.schedule.post as usize && ni == program.logits as usize;
                    commit_tiles.push(if logits { PALW_LOGITS_TILE_LANES as u32 } else { 8 });
                }
            }
        }
        let layout = PalwTirLayoutV1 {
            version: PALW_TIR_LAYOUT_VERSION_V1,
            max_context: 64,
            checkpoint_interval: 2,
            h_tile: 4,
            commit_tiles,
            state_tiles: program.states.iter().map(|_| 4).collect(),
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dense.palwtir");
        let meta = serde_json::json!({ "model_id": "test/tn1-dense-gqa-2layer" }).to_string();
        misaka_palw_tir_artifact::write_container_v1(&path, &program, borsh::to_vec(&layout).unwrap(), [9; 64], meta, &mut |j, l| {
            tensors.get(&(j, l)).cloned().ok_or_else(|| format!("no tensor {j} {l:?}"))
        })
        .expect("the container");
        let holding = Self::sdk(chain).load_artifact(&path).expect("the IR lineage loads it by its magic");
        let entry = misaka_palw_sdk::tir_registration::tir_entries_of_v1(std::slice::from_ref(&holding)).remove(0);
        let (class_id, root) = (entry.class_id(), entry.artifact_root);
        TirFixture { _dir: dir, path, entry, class_id, root, layout }
    }

    fn sdk(chain: &Chain) -> misaka_palw_sdk::PalwClassSdk {
        misaka_palw_sdk::PalwClassSdk::builtin_v1(
            chain.bundle.court,
            chain.config.params.palw_prompt_ids_form_v1(),
            chain.config.params.net.to_string().into_bytes(),
        )
        .with_chain_classes_v1()
    }

    /// The class's backend as a node resolves it (the node's registry over the deployed file).
    pub fn backend(&self, chain: &Chain) -> Box<dyn PalwExecutionBackendV1> {
        let holding = Self::sdk(chain).load_artifact(&self.path).unwrap();
        let registry = crate::palw_backends::PalwBackendRegistry::for_node_v1(
            &chain.config.params,
            chain.bundle.court,
            chain.config.params.palw_prompt_ids_form_v1(),
            vec![holding],
            chain.config.params.net.to_string().into_bytes(),
            true,
        );
        registry.resolve(self.class_id, self.root).expect("the node resolves the deployed IR class")
    }

    /// **Piece 1a: the registration** — the SDK's builder over the chain's own terms, gated by the node's preflight, signed by card 0
    /// (the registrant) and carried.
    pub async fn register(&self, chain: &mut Chain, floats: &mut [Funding]) {
        let terms = chain.tc.palw_v2_registration_terms().expect("the chain's registration terms");
        let activation = chain.daa() + 4;
        let build = |signature: Vec<u8>, chain: &Chain| {
            misaka_palw_sdk::tir_registration::build_tir_registration_v1(
                &chain.config.params,
                &chain.bundle,
                &self.entry,
                &terms,
                activation,
                chain.bonds[0],
                signature,
                chain.daa(),
            )
            .expect("the node's preflight admits the class")
        };
        let unsigned = build(Vec::new(), chain);
        let message = misaka_palw_sdk::tir_registration::tir_registration_message_v1(chain.network_domain, &unsigned).unwrap();
        let object = build(
            signed(0, message.as_byte_slice(), kaspa_consensus_core::palw_tir_class_v1::PALW_TIR_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1),
            chain,
        );
        chain.send(0, &object, &mut floats[0], &[]).await;
        assert!(chain.tc.palw_tir_class_record_v1(self.class_id).is_some(), "the IR class registered through the mempool");
        eprintln!("[service-tir-frame] IR class {} registered at DAA {}", self.class_id, chain.daa());
    }

    /// **Piece 2: the family and lane certification and the layout, through the mempool** — `FamilyCertified { TirAttempt }` (the
    /// class's own drill, graded by the chain), `ClassLaneCertifiedTirV1`, `TirClassLayoutPublishedV1`.
    pub async fn certify_and_publish(&self, chain: &mut Chain, floats: &mut [Funding]) {
        let (certificate, family) = misaka_palw_sdk::tir_certification::tir_family_certification_v1(
            &self.entry,
            &chain.bundle.court,
            chain.config.params.palw_prompt_ids_form_v1(),
            None,
        )
        .expect("the class's drill certifies");
        let carriers =
            kaspa_consensus_core::palw_state_v2::palw_object_chunks_v1(&certificate).unwrap().unwrap_or_else(|| vec![certificate]);
        let count = carriers.len();
        for carrier in carriers {
            chain.send(0, &carrier, &mut floats[0], &[]).await;
        }
        assert!(
            chain.tc.palw_certified_families_v1().iter().any(|(_, _, row)| row.family.family_id == family.family_id),
            "the IR family is certified on chain"
        );
        let lane = misaka_palw_sdk::tir_certification::tir_lane_certification_v1(&self.entry);
        chain.send(0, &lane, &mut floats[0], &[]).await;
        let layout =
            PalwConsensusObjectV2::TirClassLayoutPublishedV1 { class_id: self.class_id, layout: Box::new(self.layout.clone()) };
        chain.send(0, &layout, &mut floats[0], &[]).await;
        eprintln!("[service-tir-frame] family certified ({count} carriers), lane certified, layout published at DAA {}", chain.daa());
    }

    /// One seat's readiness V2 proof for the current span, built as the node's readiness duty builds it.
    pub fn readiness(&self, chain: &Chain, card: usize) -> PalwConsensusObjectV2 {
        use kaspa_consensus_core::palw_model_registry_v1::*;
        let read = chain.tc.palw_model_registry_v1().expect("the registry is active");
        let span = chain.daa() / read.span_daa;
        let bond = chain.bonds[card];
        let bond_bytes = borsh::to_vec(&bond).unwrap();
        let backend = self.backend(chain);
        let (root, leaves) = backend.artifact_root_and_leaf_count().unwrap();
        assert_eq!(root, self.root);
        let draw = palw_readiness_v2_draw_v1(&palw_readiness_v2_challenge_seed_v1(&self.class_id, &bond_bytes, span), leaves);
        let (_, all, drawn) = backend.artifact_readiness_material(&draw).expect("the drawn leaves open");
        let mut opened = Vec::new();
        let mut bytes = 0usize;
        for (index, operand) in drawn {
            if bytes >= PALW_READINESS_V2_BUDGET_BYTES_V1 {
                break;
            }
            bytes += operand.bytes.len();
            opened.push((index, operand));
        }
        opened.sort_by_key(|(index, _)| *index);
        let proof = kaspa_consensus_core::palw_artifact::palw_artifact_multiproof_v1(&all, &opened).expect("a multiproof");
        kaspa_consensus_core::palw_artifact::verify_artifact_multiproof_v1(&proof, self.root).expect("it opens the class root");
        let message = palw_seat_readiness_message_v2(chain.network_domain, &bond_bytes, &self.class_id, span, &proof);
        PalwConsensusObjectV2::SeatReadinessProvedV2 {
            bond,
            class_id: self.class_id,
            span,
            proof: Box::new(proof),
            signature: signed(card, message.as_byte_slice(), PALW_SEAT_READINESS_V2_MLDSA87_CONTEXT),
        }
    }

    /// Every seat but the registrant re-proves readiness for the current span — all seven carriers in ONE block (a span is short on
    /// this network, and a proof carried a span late is stale).
    pub async fn refresh_readiness(&self, chain: &mut Chain, floats: &mut [Funding]) {
        let items: Vec<(usize, PalwConsensusObjectV2)> = (1..chain.bonds.len()).map(|card| (card, self.readiness(chain, card))).collect();
        send_batch(chain, floats, items).await;
    }

    pub fn lifecycle(&self, chain: &Chain) -> Option<PalwModelLifecycleV1> {
        chain.tc.palw_model_registry_v1()?.classes.iter().find(|c| c.class_id == self.class_id)?.row.as_ref().map(|r| r.state)
    }

    /// **Piece 1b: the admission jury** — every seat declares the class capable and proves possession ahead of the class's staggered
    /// audit; a winning floor attempt anchors the jury; then the ready seats move the class `Prefetching → Probation`.
    pub async fn seat_jury(&self, chain: &mut Chain, floats: &mut [Funding]) {
        for card in 1..chain.bonds.len() {
            let capable_classes = BTreeSet::from([chain.bundle.base_class_id, self.class_id]);
            let message = kaspa_consensus_core::palw_state_v2::palw_bond_capability_message_v2(
                chain.network_domain,
                &chain.bonds[card],
                &capable_classes,
            );
            let object = PalwConsensusObjectV2::BondCapabilityDeclared {
                bond: chain.bonds[card],
                capable_classes,
                signature: signed(
                    card,
                    message.as_byte_slice(),
                    kaspa_consensus_core::palw_state_v2::PALW_BOND_CAPABILITY_V2_MLDSA87_CONTEXT,
                ),
            };
            chain.send(card, &object, &mut floats[card], &[]).await;
        }
        let read = chain.tc.palw_model_registry_v1().unwrap();
        let period = kaspa_consensus_core::palw_model_registry_v1::palw_admission_audit_period_spans_v2(
            chain.bundle.state.epoch_length(),
            read.span_daa,
            chain.config.params.palw_admission_audit_period_daa,
        );
        let span = chain.daa() / read.span_daa;
        let audit = (span + 12..=span + 12 + period)
            .find(|s| kaspa_consensus_core::palw_activation_pool_v1::palw_admission_audit_due_staggered_v1(&self.class_id, *s, period))
            .unwrap();
        chain.advance((audit - 10) * read.span_daa).await;
        self.refresh_readiness(chain, floats).await;
        chain.advance((audit - 1) * read.span_daa).await;
        let (anchor, _) = chain.build_attempt_drawn(0, chain.config.params.target_time_per_block(), vec![], &|_| true, true);
        chain.insert(anchor).await;
        chain.advance((audit + 1) * read.span_daa).await;
        eprintln!("[service-tir-frame] jury at span {audit}: {:?}", self.lifecycle(chain));
        assert!(!matches!(self.lifecycle(chain), Some(PalwModelLifecycleV1::Candidate) | None), "the outside jury seats the class");
        let mut guard = 0;
        while !matches!(self.lifecycle(chain), Some(PalwModelLifecycleV1::Probation { .. })) {
            guard += 1;
            assert!(guard < 8, "the ready seats move the class to Probation: {:?}", self.lifecycle(chain));
            self.refresh_readiness(chain, floats).await;
            let next = (chain.daa() / read.span_daa + 1) * read.span_daa;
            chain.advance(next + 1).await;
        }
        eprintln!("[service-tir-frame] {:?} at DAA {}", self.lifecycle(chain), chain.daa());
    }

    /// **A probation probe of the IR class by card `card`, mined through the production template** (piece 1c). As the parent harness
    /// does for its floor attempts, the probe commits roots it did not compute and is licensed by the harness's colluding Panel: the
    /// class lottery prices a win at `W / CCU` forwards (≈ 5.2 M for this tiny class on testnet-12, ~7 h of debug-build inference),
    /// so the walk draws the ticket over the attempt's own free fields (its trace root) instead. The registry's probation counts a
    /// probe that goes `Final`; nothing about the frame route below rests on these probes. Returns the claim id.
    pub async fn ir_attempt(&self, chain: &mut Chain, card: usize) -> Option<Hash64> {
        use kaspa_consensus_core::palw_attempt_v2::*;
        chain.time += chain.config.params.target_time_per_block();
        chain.nonce += 1;
        let bond = chain.bonds[card];
        let mut t = chain
            .tc
            .build_block_template(
                MinerData::new(card_payout_spk(card), vec![]),
                Box::new(OnetimeTxSelector::new(vec![])),
                TemplateBuildMode::Standard,
            )
            .expect("a template");
        t.block.header.timestamp = chain.time;
        t.block.header.nonce = chain.nonce;
        let facts = chain.tc.palw_producer_facts_v2(self.class_id, Some(bond.0)).expect("the chain answers for the IR class");
        let bond_facts = facts.bond.as_ref().expect("a genesis card").clone();
        let header = &t.block.header;
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
        let mut attempt = PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: chain.network_domain,
            challenge: challenge_v2(chain.network_domain, pre_pow, header.timestamp, header.nonce, self.class_id, &bond.0),
            class_id: self.class_id,
            executor_bond: bond.0,
            executor_pubkey: card_pubkey(card),
            operator_id: bond_facts.operator_id,
            artifact_root: self.root,
            trace_root: Hash64::default(),
            output_root: Hash64::from_u64_word(0x0071_0000_0000_0000 | chain.nonce),
            execution_root: Hash64::from_u64_word(0xE71C_0000_0000_0000 | chain.nonce),
            pwu: facts.pwu,
            trace_manifest_root: Hash64::default(),
            trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
            trace_retention_daa: header.daa_score.saturating_add(facts.min_trace_retention_daa),
        };
        let anchor = execution_anchor_v3(chain.network_domain, pre_pow, self.class_id, &bond.0, header.nonce);
        let draws = (u128::MAX / facts.class_target.max(1)).saturating_mul(40).min(400_000_000) as u64;
        for draw in 0u64..draws {
            attempt.trace_root = Hash64::from_u64_word((chain.nonce << 40) ^ draw ^ 0x7B00_0000_0000_0000);
            attempt.trace_manifest_root = attempt_trace_manifest_root_v1(attempt.trace_root, attempt.trace_chunk_count);
            if class_ticket_v3(&attempt, anchor) > facts.class_target {
                continue;
            }
            let claim = attempt_id_v2(&attempt);
            let signature = libcrux_ml_dsa::ml_dsa_87::sign(
                &key(card).signing_key,
                claim.as_byte_slice(),
                PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
                [0x5A; 32],
            )
            .unwrap()
            .as_ref()
            .to_vec();
            t.block.header.palw_commitment = PalwAttemptEnvelopeV2 { attempt, signature }.encode_wire();
            t.block.header.finalize();
            chain.insert(t.block).await;
            let accepted = chain.tc.palw_legacy_dispute_v1(claim).is_some();
            eprintln!(
                "[service-tir-frame] IR probe {claim} won at draw {draw}, DAA {}, accepted {accepted}, lifecycle {:?}",
                chain.daa(),
                self.lifecycle(chain)
            );
            return accepted.then_some(claim);
        }
        panic!("the IR class lottery is winnable within {draws} draws");
    }

    /// The colluding Panel licenses `claim` (a V2 quorum of every bound seat's Valid, assembled by the chain's own assembler).
    pub async fn license(&self, chain: &mut Chain, floats: &mut [Funding], claim: Hash64) {
        let duties = chain.tc.palw_seat_duties_v2(chain.bonds.clone()).into_iter().filter(|d| d.claim_id == claim).collect::<Vec<_>>();
        assert!(!duties.is_empty(), "the claim's Panel is bound: {:?}", chain.view(claim));
        let signed_daa = chain.daa();
        let receipts = duties
            .iter()
            .map(|d| {
                let card = chain.bonds.iter().position(|b| *b == d.seat_bond).unwrap();
                let message = palw_receipt_message_v2(chain.network_domain, claim, PalwReceiptVerdictV2::Valid, signed_daa);
                PalwSeatReceiptV2 {
                    claim,
                    verdict: PalwReceiptVerdictV2::Valid,
                    seat_bond: d.seat_bond,
                    signed_daa,
                    signature: signed(card, message.as_byte_slice(), PALW_RECEIPT_V2_MLDSA87_CONTEXT),
                }
            })
            .collect();
        let object = chain.tc.palw_v2_receipt_quorum_assemble(claim, receipts).expect("the Panel licenses");
        let card = chain.bonds.iter().position(|b| *b == duties[0].seat_bond).unwrap();
        chain.send(card, &object, &mut floats[card], &[]).await;
    }

    /// **Piece 1: the whole walk to `Active` through the mempool and the production template** — registration, certification,
    /// layout, jury, then the registry's probation (IR attempts bound to the network's Panels, licensed, `Final`) and its stable spans.
    pub async fn onboard_to_active(&self, chain: &mut Chain, floats: &mut [Funding]) {
        self.register(chain, floats).await;
        self.certify_and_publish(chain, floats).await;
        eprintln!("[service-tir-frame] lifecycle after registration: {:?}", self.lifecycle(chain));
        self.seat_jury(chain, floats).await;
        let read = chain.tc.palw_model_registry_v1().unwrap();
        let probes = read.globals.expect("the registry globals").probation_claims as usize;
        eprintln!(
            "[service-tir-frame] span {} DAA, readiness horizon {:?} spans, anchor delay {}",
            read.span_daa,
            read.globals.map(|g| g.readiness_v2_max_age_spans),
            chain.bundle.panel.anchor_delay()
        );
        let mut guard = 0;
        while matches!(self.lifecycle(chain), Some(PalwModelLifecycleV1::Probation { .. })) {
            guard += 1;
            assert!(guard < 6, "the probation ends: {:?}", self.lifecycle(chain));
            // One probe at a time: the class's admission (Probation's panel budget) and the seats' room bound how many probes
            // may be bound at once, so each is drawn, bound and licensed before the next (a probe the draw cannot seat voids
            // `NoCapablePanel` and is simply not counted).
            let mut claims = Vec::new();
            let mut tries = 0;
            while claims.len() < probes {
                tries += 1;
                assert!(tries < probes * 4, "the probation's probes keep failing to bind");
                self.refresh_readiness(chain, floats).await;
                let Some(claim) = self.ir_attempt(chain, 0).await else {
                    // The class gate holds this probe (e.g. `Held` after an unseated draw): the ready seats re-seat it next span.
                    let next = (chain.daa() / read.span_daa + 1) * read.span_daa;
                    chain.advance(next + 1).await;
                    continue;
                };
                let slot = chain.view(claim).accepted_daa + chain.bundle.panel.anchor_delay();
                chain.advance(slot.saturating_sub(2)).await;
                self.refresh_readiness(chain, floats).await;
                chain.advance(slot).await;
                let (anchor, _) = chain.build_attempt_drawn(7, chain.config.params.target_time_per_block(), vec![], &|_| true, true);
                chain.insert(anchor).await;
                if chain.tc.palw_seat_duties_v2(chain.bonds.clone()).iter().any(|d| d.claim_id == claim) {
                    self.license(chain, floats, claim).await;
                    claims.push(claim);
                } else {
                    eprintln!("[service-tir-frame] probe {claim} not bound: {:?}", chain.view(claim).phase);
                }
            }
            let mut waits = 0;
            while !claims.iter().all(|c| matches!(chain.view(*c).phase, PalwClaimPhaseV2::Final { .. })) {
                waits += 1;
                assert!(waits < 60, "the licensed probes reach Final: {:?}", claims.iter().map(|c| chain.view(*c).phase).collect::<Vec<_>>());
                self.refresh_readiness(chain, floats).await;
                let next = (chain.daa() / read.span_daa + 1) * read.span_daa;
                chain.advance(next.min(chain.daa() + 40)).await;
            }
            for claim in &claims {
                assert!(
                    matches!(chain.view(*claim).phase, PalwClaimPhaseV2::Final { .. }),
                    "probe {claim}: {:?}",
                    chain.view(*claim).phase
                );
            }
            let next = (chain.daa() / read.span_daa + 1) * read.span_daa;
            chain.advance(next + 1).await;
            eprintln!("[service-tir-frame] after {probes} probe Finals: {:?} at DAA {}", self.lifecycle(chain), chain.daa());
        }
        let mut guard = 0;
        while !matches!(self.lifecycle(chain), Some(PalwModelLifecycleV1::Active)) {
            guard += 1;
            assert!(guard < 12, "the walk stalls at {:?}", self.lifecycle(chain));
            self.refresh_readiness(chain, floats).await;
            let next = (chain.daa() / read.span_daa + 1) * read.span_daa;
            chain.advance(next + 1).await;
        }
        eprintln!("[service-tir-frame] the IR class is Active at DAA {}", chain.daa());
    }
}

/// **Pieces 1 and 2**: an IR class walks to `Active` on testnet-12's ruleset through the PQ mempool and the production template.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tir_frame_node_an_ir_class_walks_to_active_through_the_mempool() {
    on_service_stack(|| async {
        kaspa_core::log::try_init_logger("warn");
        let (config, bundle, premine, mut floats) = tir_frame_config();
        let mut chain = Chain::new(config, bundle, &premine);
        chain.advance(2).await;
        let fixture = TirFixture::new(&chain);
        fixture.onboard_to_active(&mut chain, &mut floats).await;
        assert!(chain.tc.palw_tir_class_record_v1(fixture.class_id).is_some());
    })
    .await;
}

/// The IR class's lottery odds and one draw's cost on this build (a sizing probe for the walk; prints, asserts nothing else).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tir_frame_node_lottery_odds_probe() {
    on_service_stack(|| async {
        kaspa_core::log::try_init_logger("warn");
        let (config, bundle, premine, mut floats) = tir_frame_config();
        let mut chain = Chain::new(config, bundle, &premine);
        chain.advance(2).await;
        let fixture = TirFixture::new(&chain);
        fixture.register(&mut chain, &mut floats).await;
        fixture.certify_and_publish(&mut chain, &mut floats).await;
        chain.advance(chain.daa() + 8).await;
        for class in [chain.bundle.base_class_id, fixture.class_id] {
            let facts = chain.tc.palw_producer_facts_v2(class, Some(chain.bonds[0].0)).expect("facts");
            eprintln!(
                "[service-tir-frame] class {class}: target {} (p = {:.3e}), pwu {}",
                facts.class_target,
                facts.class_target as f64 / u128::MAX as f64,
                facts.pwu
            );
        }
        let backend = fixture.backend(&chain);
        let start = std::time::Instant::now();
        for i in 0..20u64 {
            let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(i)).unwrap();
            let job = kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1(job, true);
            backend.execute(&job, &prompt).unwrap();
        }
        eprintln!("[service-tir-frame] one IR draw: {:?}", start.elapsed() / 20);
    })
    .await;
}

// ---- the node: the actual services over the actual mempool and template --------------------------------------------------------

/// An ordinary public bond registered now (the outside verifier), from testnet-12's main premine re-addressed to its key.
pub(super) async fn register_outsider(chain: &mut Chain, floats: &mut [Funding]) -> PalwBondKeyV2 {
    let signed_bond = PalwBondKeyV2(TransactionOutpoint::new(Default::default(), 0));
    let public_key = card_pubkey(OUTSIDER);
    let payout = card_payout_payload(OUTSIDER);
    let classes = BTreeSet::new();
    let collateral = PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
    let message =
        palw_bond_registration_message_v2(chain.network_domain, &signed_bond, &public_key, &public_key, collateral, &payout, &classes);
    let possession = palw_operator_possession_message_v1(chain.network_domain, &signed_bond, &public_key, &public_key);
    let mut signature = signed(OUTSIDER, message.as_byte_slice(), PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT);
    signature.extend(signed(OUTSIDER, possession.as_byte_slice(), PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT));
    let registration = PalwConsensusObjectV2::BondRegistered {
        bond: signed_bond,
        pubkey: public_key.clone(),
        operator_pubkey: public_key.clone(),
        collateral,
        payout_payload: payout,
        capable_classes: classes,
        signature,
    };
    let tx = chain
        .send(OUTSIDER, &registration, &mut floats[OUTSIDER], &[TransactionOutput::new(collateral, card_payout_spk(OUTSIDER))])
        .await;
    let bond = PalwBondKeyV2(TransactionOutpoint::new(tx.id(), 0));
    assert_eq!(chain.tc.palw_bond_of_pubkey_v2(&public_key).unwrap().0, bond);
    let maturity = chain.config.params.palw_bond_maturity.unwrap().window_daa;
    chain.advance(chain.daa() + maturity + 1).await;
    bond
}

/// The service configuration of card `card` (its own key, its fee outpoint, the deployed IR class file).
fn service_config(chain: &Chain, dir: &std::path::Path, card: usize, bond: PalwBondKeyV2, funding: &Funding, fixture: &TirFixture) -> PalwPanelConfig {
    std::fs::create_dir_all(dir).unwrap();
    let mut config = panel_config(chain, dir, bond, funding);
    let mut seed = [0xB0; 32];
    seed[0] += card as u8;
    std::fs::write(&config.key_path, faster_hex::hex_string(&seed)).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&config.key_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    config.class_artifacts = vec![fixture.path.clone()];
    config.chain_classes = true;
    config.retention_dir = dir.join("retention");
    config
}

/// The node's service stack: the consensus manager, the mempool, the flow context and the running services.
pub(super) struct Node {
    pub manager: Arc<ConsensusManager>,
    pub pool: MiningManagerProxy,
    flow: Arc<FlowContext>,
    tick: Arc<TickService>,
    pub services: Vec<ServiceRun>,
    _meta: kaspa_database::utils::DbLifetime,
}

impl Node {
    pub fn new(chain: &Chain) -> Self {
        let manager = Arc::new(ConsensusManager::new(Arc::new(TestConsensusFactory::new(chain.tc.clone()))).unwrap());
        let config = Arc::new(chain.config.clone());
        let tick = Arc::new(TickService::new());
        let (meta_lifetime, meta_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let (addresses, _) = kaspa_addressmanager::AddressManager::new(config.clone(), meta_db, tick.clone());
        let pool = MiningManagerProxy::new(Arc::new(MiningManager::new_with_extended_config(
            config.target_time_per_block(),
            false,
            config.max_block_mass,
            config.ram_scale,
            config.block_template_cache_lifetime,
            Default::default(),
            true,
            config.palw_model_market.is_some(),
            config.palw_activation_pool_fence().is_some(),
            None,
            AttestationMempoolPolicy::disabled(),
            kaspa_mining::mempool::config::palw_h1_carrier_priority_for(&config.params),
        )));
        let hub = kaspa_p2p_lib::Hub::new();
        let rules = Arc::new(MiningRuleEngine::new(
            manager.clone(),
            config.clone(),
            Default::default(),
            tick.clone(),
            hub.clone(),
            Default::default(),
            Arc::new(kaspa_core::chain_participation::ChainParticipationGate::new(true)),
        ));
        let flow =
            Arc::new(FlowContext::new(manager.clone(), addresses, config, pool.clone(), tick.clone(), chain.tc.notification_root(), hub, rules));
        Node { manager, pool, flow, tick, services: Vec::new(), _meta: meta_lifetime }
    }

    pub fn start(&mut self, chain: &Chain, config: PalwPanelConfig) {
        let service = Arc::new(PalwPanelService::new(config, self.manager.clone(), self.flow.clone(), Arc::new(chain.config.clone())));
        let cloned = service.clone();
        let task = Some(tokio::spawn(async move { cloned.start().await.unwrap() }));
        self.services.push(ServiceRun { service, task });
    }

    /// One block of the production template over the mempool the services feed (two when carriers were pending).
    pub async fn step(&self, chain: &mut Chain) {
        for run in &self.services {
            assert!(run.task.as_ref().is_some_and(|task| !task.is_finished()), "a production service unexpectedly stopped");
        }
        let (pending, _) = self.pool.clone().get_all_transactions(TransactionQuery::TransactionsOnly).await;
        pool_heartbeat(chain, &self.manager, &self.pool).await;
        if !pending.is_empty() {
            pool_heartbeat(chain, &self.manager, &self.pool).await;
        }
        tokio::time::sleep(Duration::from_millis(60)).await;
    }

    pub async fn finish(mut self) {
        for run in &mut self.services {
            run.finish().await;
        }
        self.tick.shutdown();
    }
}

/// The TFP1-framed free-prompt claims producer card 0 carried in the actual blocks (the envelope's pledge names the claim).
fn framed_claims(chain: &Chain) -> Vec<Hash64> {
    chain
        .history
        .iter()
        .flat_map(|b| b.transactions.iter())
        .filter_map(|t| kaspa_consensus_core::palw_tir_public_fp_envelope_v1::PalwTirPublicFpEnvelopeV1::decode_v1(&t.payload).ok())
        .map(|envelope| envelope.pledge.claim)
        .collect()
}

/// A winning floor attempt (card 7), the jury/panel anchor a bound Panel is drawn at.
async fn anchor_block(chain: &mut Chain) {
    let (anchor, _) = chain.build_attempt_drawn(7, chain.config.params.target_time_per_block(), vec![], &|_| true, true);
    chain.insert(anchor).await;
}

/// **One quantum of `claim` spent into a receipt block** (the producer's payment: the receipt lane's block, as
/// `palw_producer::produce_receipt` builds it). `None` while no quantum of the claim wins inside its use window.
async fn spend_one_quantum(chain: &mut Chain, card: usize, claim: Hash64) -> Option<Hash64> {
    let bond = chain.bonds[card].0;
    let next_daa = chain.daa();
    let rows = chain.tc.palw_fp_spendable_v3(bond);
    eprintln!(
        "[service-tir-frame] spendable quanta of {claim}: {:?}",
        rows.iter().filter(|q| q.claim_id == claim).map(|q| (q.quantum_index, q.wins, q.spend_deadline_daa)).collect::<Vec<_>>()
    );
    let win = rows.into_iter().find(|q| q.claim_id == claim && q.wins && next_daa <= q.spend_deadline_daa)?;
    let mut template = chain
        .tc
        .build_block_template(MinerData::new(card_payout_spk(card), vec![]), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
        .unwrap();
    chain.stamp(&mut template.block);
    template.block.header.pow_algo_id = kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3;
    let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&template.block.header);
    let mut seed = [0xB0; 32];
    seed[0] += card as u8;
    let key = kaspa_pq_validator_core::ValidatorKey::from_seed(seed);
    let envelope = key.build_fp_receipt_spend_envelope(
        chain.network_domain,
        pre_pow,
        template.block.header.timestamp,
        template.block.header.nonce,
        win.claim_id,
        win.quantum_index,
        bond,
        win.beacon.beacon_block,
    );
    template.block.header.palw_commitment = envelope.encode();
    template.block.header.finalize();
    let block = chain.insert(template.block).await;
    Some(block.hash())
}

/// **The colluding Panel's own objects for a forged framed claim** (the adversary is harness-scripted; honest software would refuse to
/// sign): each bound seat files ONE preparation debit for its pass and its atomic Valid + raw custody grant + Ready, then the Panel
/// licenses on exactly those credited receipts — every object through the PQ mempool and the production template.
async fn colluders_license(chain: &mut Chain, floats: &mut [Funding], claim: Hash64) -> Vec<usize> {
    use kaspa_consensus_core::palw_tir_public_carriage_v1::{
        PALW_TIR_PUBLIC_CONTROL_CONTEXT_V1, PALW_TIR_PUBLIC_GRANT_READY_CONTEXT_V1, PalwTirPublicControlActionV1 as A,
        PalwTirPublicControlTargetV1 as T, PalwTirPublicControlV1, PalwTirPublicGrantReadyV1,
    };
    use kaspa_consensus_core::palw_tir_public_owned_v1::{PalwTirPublicWorkPhaseV1, PalwTirPublicWorkScopeV1};
    let duties = chain.tc.palw_seat_duties_v2(chain.bonds.clone()).into_iter().filter(|d| d.claim_id == claim).collect::<Vec<_>>();
    assert!(!duties.is_empty(), "the forged claim's Panel is bound: {:?}", chain.view(claim));
    let seats: Vec<usize> = duties.iter().map(|d| chain.bonds.iter().position(|b| *b == d.seat_bond).unwrap()).collect();
    // 1. Each seat's one pass debit, prepared on the accepted state through the consensus work book.
    let mut debits = Vec::new();
    let mut frame_out = None;
    chain
        .tc
        .palw_tir_public_read_v1(&mut |state| {
            let frame = kaspa_consensus_core::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim).ok_or("no frame")?.clone();
            let plan = frame.plan_v1().clone();
            let key = kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedCourtKeyV1::TirPreparation(claim, plan.allocation_id);
            let maximum = frame.descriptor_v1().maximum_leaves;
            for card in &seats {
                let actor = chain.bonds[*card];
                let mut book = kaspa_consensus_core::palw_tir_public_state_v1::palw_tir_public_work_book_v1(state, key, actor, maximum)?;
                let snapshot =
                    kaspa_consensus_core::palw_tir_public_state_v1::palw_tir_public_preparation_snapshot_v1(state, key, maximum)?.ok_or("no row")?;
                let lane = &snapshot.row.preparation.work_v1().allocation;
                let work = kaspa_consensus_core::palw_tir_frame_cost_v1::tir_public_pass_budget_v1(lane.per_actor, lane.maximum_restarts + 1)?;
                let producer = PalwBondKeyV2(frame.original_v1().job.executor_bond);
                let scope = PalwTirPublicWorkScopeV1 {
                    claim,
                    owner: producer,
                    reserved_daa: plan.terms.accepted_daa,
                    bundle_id: plan.allocation_id,
                    source_id: plan.source_id,
                    turn: 0,
                    request: kaspa_consensus_core::palw_tir_public_source_v1::PalwTirPublicRequestV1::StepLeaf { index: 0 },
                    actor,
                    phase: PalwTirPublicWorkPhaseV1::Capture,
                    input_head: plan.source_id,
                    input_offset: 0,
                    slice: 0,
                    cursor_id: Hash64::from_u64_word(0xC0 + *card as u64),
                };
                debits.push((*card, book.prepare_v1(state, scope, work, None, maximum)?));
            }
            frame_out = Some(frame);
            Ok(())
        })
        .expect("the accepted TIR state reads");
    let frame = frame_out.expect("the frame");
    let plan = frame.plan_v1().clone();
    for (card, action) in debits {
        let mut control = PalwTirPublicControlV1 {
            version: 1,
            claim,
            allocation_id: plan.allocation_id,
            source_id: plan.source_id,
            target: T::Preparation { original_accepted_daa: plan.terms.accepted_daa },
            expected_turn: 0,
            declared_rent_sompi: 0,
            signer: chain.bonds[card],
            action: A::Preparation(action),
            signature: Vec::new(),
        };
        let message = control.signing_message_v1(chain.network_domain).unwrap();
        control.signature = signed(card, message.as_byte_slice(), PALW_TIR_PUBLIC_CONTROL_CONTEXT_V1);
        chain.send(card, &PalwConsensusObjectV2::TirPublicControlV1 { control: Box::new(control) }, &mut floats[card], &[]).await;
    }
    // 2. Each seat's atomic Valid + grant + Ready (a descriptor it never prepared: it signs whatever the producer committed).
    let header = frame.header_v1();
    let original = frame.original_v1();
    let descriptor = kaspa_consensus_core::palw_backend::PalwTirPublicPreparedDescriptorV1 {
        version: 1,
        source_id: plan.source_id,
        class_id: frame.descriptor_v1().class_id,
        artifact_root: header.artifact_root,
        execution_root: original.execution_root,
        trace_root: original.trace_root,
        step_root: header.step_merkle_root,
        step_leaf_count: header.step_leaf_count,
        decode_count: header.job_context.exact_decode_tokens,
        vocab: plan.acquisition.envelope.vocab,
        material_until_daa: plan.terms.material_until_daa,
        manifest_digest: Hash64::from_u64_word(0xD1),
        retained_bytes: 1,
    };
    let mut receipts = Vec::new();
    for card in &seats {
        let seat = chain.bonds[*card];
        let signed_daa = chain.daa();
        let message = palw_receipt_message_v2(chain.network_domain, claim, PalwReceiptVerdictV2::Valid, signed_daa);
        let receipt = PalwSeatReceiptV2 {
            claim,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: seat,
            signed_daa,
            signature: signed(*card, message.as_byte_slice(), PALW_RECEIPT_V2_MLDSA87_CONTEXT),
        };
        let mut grant = kaspa_consensus_core::palw_tir_public_custody_v1::PalwTirPublicCustodyGrantV1 {
            version: 1,
            claim,
            seat,
            signed_daa,
            pledge_id: frame.custody_v1().pledge_v1().id_v1(),
            valid_receipt_digest: kaspa_consensus_core::palw_tir_public_custody_v1::PalwTirPublicCustodyGrantV1::receipt_digest_v1(&receipt),
            signature: Vec::new(),
        };
        grant.signature =
            signed(*card, grant.id_v1().as_byte_slice(), kaspa_consensus_core::palw_tir_public_custody_v1::PALW_TIR_SOURCE_GRANT_CONTEXT_V1);
        let mut movement = PalwTirPublicGrantReadyV1 {
            version: 1,
            claim,
            allocation_id: plan.allocation_id,
            source_id: plan.source_id,
            signer: seat,
            receipt: receipt.clone(),
            grant,
            descriptor,
            signature: Vec::new(),
        };
        let digest = movement.signing_message_v1(chain.network_domain).unwrap();
        movement.signature = signed(*card, digest.as_byte_slice(), PALW_TIR_PUBLIC_GRANT_READY_CONTEXT_V1);
        movement.check_v1().unwrap();
        chain.send(*card, &PalwConsensusObjectV2::TirPublicGrantReadyV1 { grant: Box::new(movement) }, &mut floats[*card], &[]).await;
        receipts.push(receipt);
    }
    // 3. The licence on exactly those credited receipts.
    let object = chain.tc.palw_v2_receipt_quorum_assemble(claim, receipts).expect("the colluding Panel licenses");
    chain.send(seats[0], &object, &mut floats[seats[0]], &[]).await;
    assert!(matches!(chain.view(claim).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "{:?}", chain.view(claim).phase);
    seats
}

/// **T-N1 at node level, both cases.** `forged`: the producer's service commits a cone lie (drill D-05), its whole Panel colludes,
/// and one public bond outside the Panel — the actual service — localizes and convicts.
async fn tn1_node(forged: bool) {
    kaspa_core::log::try_init_logger("warn,kaspad_lib::palw_panel=info");
    let (config, bundle, premine, mut floats) = tir_frame_config();
    let mut chain = Chain::new(config, bundle, &premine);
    chain.advance(2).await;
    let fixture = TirFixture::new(&chain);
    fixture.onboard_to_active(&mut chain, &mut floats).await;
    let outsider = register_outsider(&mut chain, &mut floats).await;
    fixture.refresh_readiness(&mut chain, &mut floats).await;
    let dirs = tempfile::tempdir().unwrap();
    let mut node = Node::new(&chain);
    // Piece 3: the producer's actual service — canonical free-prompt claims on the IR class, published as ORIGINAL frames (`TFP1`).
    let mut producer = service_config(&chain, &dirs.path().join("producer"), 0, chain.bonds[0], &floats[0], &fixture);
    producer.canonical_claims = true;
    producer.canonical_class = Some(fixture.class_id.to_string());
    producer.canonical_interval_daa = 10_000;
    if forged {
        producer.drill_tamper_tir_frame = Some(super::super::PalwDrillTirFrameFaultV1::Cone { leaf: 5 });
    }
    node.start(&chain, producer);
    // Piece 4: the honest Panel's seats are actual services (they replay, sign, prepare, grant + Ready and license); in the forged
    // case the whole Panel colludes (harness-scripted below), so no seat service runs. The outside verifier always runs.
    if !forged {
        for card in 1..chain.bonds.len() {
            let config = service_config(&chain, &dirs.path().join(format!("seat{card}")), card, chain.bonds[card], &floats[card], &fixture);
            node.start(&chain, config);
        }
    }
    let config = service_config(&chain, &dirs.path().join("outsider"), OUTSIDER, outsider, &floats[OUTSIDER], &fixture);
    node.start(&chain, config);
    let deadline = std::time::Instant::now() + Duration::from_secs(2_400);
    // The producer's first framed claim.
    let claim = loop {
        assert!(std::time::Instant::now() < deadline, "the producer service carried no TFP1 claim");
        node.step(&mut chain).await;
        if let Some(claim) = framed_claims(&chain).into_iter().find(|c| chain.tc.palw_tir_public_frame_v1(*c).ok().flatten().is_some()) {
            break claim;
        }
    };
    let frame = chain.tc.palw_tir_public_frame_v1(claim).unwrap().unwrap();
    eprintln!("[service-tir-frame] piece 3: framed claim {claim} accepted at DAA {}: {frame:?}", chain.daa());
    let producer_bond = chain.bonds[0];
    let collateral_of = |chain: &Chain, bond: PalwBondKeyV2| {
        chain.tc.palw_claim_rows_v1(bond, kaspa_consensus_core::palw_producer_v2::PalwClaimRoleV1::Executor, false, 0).unwrap().bond.unwrap().collateral
    };
    let before = collateral_of(&chain, producer_bond);
    // The Panel is drawn at the anchor after the claim's anchor delay.
    let slot = chain.view(claim).accepted_daa + chain.bundle.panel.anchor_delay();
    while chain.daa() + 2 < slot {
        node.step(&mut chain).await;
    }
    if forged {
        // No seat service runs in the forged case: the colluding seats keep their own readiness.
        fixture.refresh_readiness(&mut chain, &mut floats).await;
    }
    while chain.daa() < slot {
        node.step(&mut chain).await;
    }
    anchor_block(&mut chain).await;
    let colluders = if forged { colluders_license(&mut chain, &mut floats, claim).await } else { Vec::new() };
    let colluder_collateral: Vec<u64> = colluders.iter().map(|c| collateral_of(&chain, chain.bonds[*c])).collect();
    loop {
        assert!(std::time::Instant::now() < deadline, "no outcome: {:?}", chain.view(claim));
        node.step(&mut chain).await;
        let view = chain.view(claim);
        match &view.phase {
            PalwClaimPhaseV2::Final { .. } => {
                assert!(!forged, "a forged claim reached Final: {view:?}");
                break;
            }
            PalwClaimPhaseV2::Voided { reason, .. } => {
                assert!(forged, "an honest claim was voided: {view:?}");
                assert_eq!(*reason, PalwVoidReasonV2::CourtFraud, "{view:?}");
                break;
            }
            _ => {}
        }
        if chain.daa() % 50 == 0 {
            eprintln!("[service-tir-frame] DAA {}: {:?}", chain.daa(), view.phase);
        }
    }
    if forged {
        assert!(collateral_of(&chain, producer_bond) < before, "the producer is slashed");
        for (card, before) in colluders.iter().zip(&colluder_collateral) {
            assert!(collateral_of(&chain, chain.bonds[*card]) < *before, "colluding seat {card} is slashed");
        }
        for _ in 0..20 {
            node.step(&mut chain).await;
        }
        assert!(spend_one_quantum(&mut chain, 0, claim).await.is_none(), "a voided claim's quanta never spend");
        eprintln!("[service-tir-frame] forged: convicted by the outside service at DAA {}", chain.daa());
    } else {
        assert_eq!(collateral_of(&chain, producer_bond), before, "the honest producer is not charged");
        let mut paid = None;
        for _ in 0..400 {
            if let Some(block) = spend_one_quantum(&mut chain, 0, claim).await {
                paid = Some(block);
                break;
            }
            node.step(&mut chain).await;
        }
        let block = paid.expect("a quantum of the Final claim spends (the producer is paid)");
        eprintln!("[service-tir-frame] honest: Final and paid in receipt block {block} at DAA {}", chain.daa());
    }
    node.finish().await;
}

/// **T-N1, honest, at node level**: the producer service's framed claim is replayed and licensed by the seats' actual services,
/// reaches `Final` and is paid (a receipt block spends one of its quanta); the outside service convicts nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tir_frame_node_tn1_an_honest_framed_claim_is_final_and_paid() {
    on_service_stack(|| tn1_node(false)).await;
}

/// **T-N1, forged, at node level**: the producer service commits a cone lie, the whole Panel colludes, and the outside public bond's
/// actual service convicts it — voided `CourtFraud`, producer and Valid seats slashed, never paid.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tir_frame_node_tn1_a_forged_framed_claim_is_convicted_by_one_outside_service() {
    on_service_stack(|| tn1_node(true)).await;
}
