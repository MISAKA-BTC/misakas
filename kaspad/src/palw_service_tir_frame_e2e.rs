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
    // Keep the test-only capacity at its generic value (64 KiB / DAA). Raising this to 64 MiB
    // shortened inclusion terms but made the owner-queue worst case admit thousands of opens per
    // DAA, pushing the derived door beyond the frozen liability horizon.
    cost.clock.maximum_door_daa = bundle.state.window_challenge_at(0) + bundle.state.window_court();
    // The outside service replays once the frame's preparation closes and opens a few DAA later (its own DAA-paced loop); the
    // TEST-ONLY 2-DAA acquisition leaves it one DAA. 4 DAA keeps the frame's collect door (which the licence waits out) inside the
    // receipt window (bind + 600).
    cost.config.acquisition_daa = 4;
    p.palw_tir_frame_cost_v1 = Some(cost);
    // **HARNESS ONLY: a work target at this tiny class's scale.** `W₀ = escrow · 10⁹ / rate`: at testnet-12's 9 MSK a G it is
    // ~3.6·10^11 CCU and the ~7·10^4-CCU IR class draws (and its quanta win) at ~2·10^-7 — so its probes could only be ground, never
    // honest, and no quantum could win in a run. A rate 2,500,000x testnet-12's puts `W₀` near 1.4·10^5 CCU: the class draws near even
    // odds, its attempted payout stays under half an escrow (the cap ceiling is 80 %), and the floor (exempt from the cap) saturates.
    if let Some(payout) = p.palw_economic_payout.as_mut() {
        payout.rate_sompi_per_giga = payout.rate_sompi_per_giga.saturating_mul(2_500_000);
    }
    eprintln!("[service-tir-frame] harness payout {:?}, work target {:?}", p.palw_economic_payout, p.palw_work_target);
    p.sync_palw_g14_gates_v1();
    // **HARNESS ONLY: the network quantum at this tiny class's scale.** The golden IR program's draw is ~68,800 ccu and the t12
    // floor's is ~21.7 M, so at t12's 8 quanta a canonical job one network quantum (~2.7 M ccu) is ~40 tiny-class jobs and every
    // tiny IR claim is `ZeroQuanta`. 512 quanta a canonical job puts the quantum (~42,300 ccu) under one tiny job; the receipt cap
    // is untouched. No shipped preset changes; this is the harness's own bundle.
    if let PalwConsensusMode::ConsensusV2(bundle) = &mut p.palw_consensus_mode {
        let cap = bundle.state.fp_max_quanta_per_receipt();
        bundle.state = bundle.state.clone().with_fp_quanta(512, cap).expect("the harness quanta");
    }
    assert!(p.validate_palw_v2().is_err(), "the shipping activation guard still refuses the frame route");
    eprintln!(
        "[service-tir-frame] frame route in force from {:?} (objective offence {:?}, R-core+ {:?}, aggregate liability {:?})",
        p.palw_tir_public_conviction_v1_fence(),
        p.palw_objective_offence,
        p.palw_rcore_plus_fence(),
        p.palw_capacity_aggregate_liability.as_ref().map(|l| l.activation)
    );
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

/// The card the probation probes are mined by — not the frame's producer (card 0): a probe's drawn roots are a harness shortcut a
/// running public filer may pursue, and none of that may touch the producer whose frame T-N1 judges.
const PROBER: usize = 6;

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
    semantic_source: Option<(Vec<u8>, kaspa_consensus_core::palw_fp_constraint_job_v1::PalwTokenTableV1)>,
}

impl TirFixture {
    pub fn new(chain: &Chain) -> Self {
        Self::new_with_semantics(chain, false)
    }

    fn new_with_semantics(chain: &Chain, declared: bool) -> Self {
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
        let semantic_source = declared.then(|| {
            let vocab: serde_json::Map<String, serde_json::Value> =
                (0..program.token_bound).map(|id| (format!("token{id}"), id.into())).collect();
            let raw = serde_json::to_vec(&serde_json::json!({"model":{"type":"BPE","vocab":vocab,"merges":[]}})).unwrap();
            let tokenizer = misaka_palw_base0::tokenizer::QwenTokenizer::from_json(&raw).unwrap();
            let table = kaspa_consensus_core::palw_fp_constraint_job_v1::PalwTokenTableV1 {
                entries: (0..program.token_bound).map(|id| tokenizer.token_bytes(id)).collect(),
                eog_token_ids: vec![program.token_bound - 1],
                tokenizer_id: misaka_palw_base0::artifact::Base0ArtifactV1::tokenizer_commitment_of(&raw),
            };
            (raw, table)
        });
        let tokenizer_id = semantic_source.as_ref().map_or([9; 64], |(_, table)| table.tokenizer_id.as_bytes());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dense.palwtir");
        let meta = serde_json::json!({ "model_id": "test/tn1-dense-gqa-2layer" }).to_string();
        misaka_palw_tir_artifact::write_container_v1(
            &path,
            &program,
            borsh::to_vec(&layout).unwrap(),
            tokenizer_id,
            meta,
            &mut |j, l| tensors.get(&(j, l)).cloned().ok_or_else(|| format!("no tensor {j} {l:?}")),
        )
        .expect("the container");
        if let Some((raw, table)) = &semantic_source {
            std::fs::write(dir.path().join("tokenizer.json"), raw).unwrap();
            std::fs::write(
                crate::palw_backends::token_table_sidecar_path_v1(&path),
                kaspa_consensus_core::palw_fp_constraint_job_v1::palw_token_table_file_encode_v1(table),
            )
            .unwrap();
        }
        let holding = Self::sdk(chain).load_artifact(&path).expect("the IR lineage loads it by its magic");
        if semantic_source.is_some() {
            crate::palw_backends::load_token_table_sidecars_v1(std::slice::from_ref(&holding)).unwrap();
        }
        let entry = misaka_palw_sdk::tir_registration::tir_entries_of_v1(std::slice::from_ref(&holding)).remove(0);
        let (class_id, root) = (entry.class_id(), entry.artifact_root);
        TirFixture { _dir: dir, path, entry, class_id, root, layout, semantic_source }
    }

    fn sdk(chain: &Chain) -> misaka_palw_sdk::PalwClassSdk {
        misaka_palw_sdk::PalwClassSdk::builtin_v1(
            chain.bundle.court,
            chain.config.params.palw_prompt_ids_form_v1(),
            chain.config.params.net.to_string().into_bytes(),
        )
        .with_chain_classes_v1()
    }

    /// 02-B: an ordinary signed declaration, with an actual raw tokenizer and rendering table.
    async fn declare_semantics(&self, chain: &mut Chain, floats: &mut [Funding]) {
        use kaspa_consensus_core::palw_class_semantics_v1::*;
        let (raw, table) = self.semantic_source.as_ref().expect("the non-default semantic fixture");
        let declaration = PalwClassSemanticsDeclarationV1 {
            version: PALW_CLASS_SEMANTICS_VERSION_V1,
            class_id: self.class_id,
            artifact_root: self.root,
            model_profile_id: Hash64::from_u64_word(0x02B1),
            runtime_manifest_hash: Hash64::from_u64_word(0x02B2),
            runtime_class_id: Hash64::from_u64_word(0x02B3),
            tokenizer_id: table.tokenizer_id,
            cu_ruleset_id: Hash64::from_u64_word(0x02B4),
            token_table_root: table.root(),
            rendering_rule: kaspa_consensus_core::palw_tokenizer_rendering_v1::PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1,
            raw_tokenizer_bytes: raw.len() as u64,
            tokenizer_key_events: kaspa_consensus_core::palw_tokenizer_rendering_v1::palw_tokenizer_key_events_v1(raw).unwrap(),
            prefix_state_relation: None,
        };
        let message = palw_class_semantics_message_v1(chain.network_domain, &declaration);
        let object = PalwConsensusObjectV2::ClassSemanticsDeclaredV1 {
            declaration: Box::new(declaration),
            signature: signed(0, message.as_byte_slice(), PALW_CLASS_SEMANTICS_SIGNATURE_CONTEXT_V1),
        };
        chain.send(0, &object, &mut floats[0], &[]).await;
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
        let mut published = None;
        let class_id = self.class_id;
        let read = chain.tc.palw_tir_public_read_v1(&mut |state| {
            published = Some(kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state, class_id).cloned());
            Ok(())
        });
        eprintln!("[service-tir-frame] layout read {read:?}: {:?}", published.as_ref().map(|l| l.is_some()));
        if read == Ok(true) {
            assert!(published.flatten().as_ref() == Some(&self.layout), "the layout row is published on chain");
        }
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

    /// An honest IR attempt through the production template and execution backend.
    /// Used for probation and for the attempt-class blocks that supply receipt beacons.
    /// Every candidate executes its own public anchor before the ordinary class lottery.
    pub async fn ir_attempt(&self, chain: &mut Chain, card: usize) -> Option<Hash64> {
        use kaspa_consensus_core::palw_attempt_v2::*;
        // **An HONEST probe**: the job its template's anchor implies, executed by the class's own backend, its roots the run's
        // (`palw_attempt_from_execution_v1`, the producer's assembly). The harness's `W` (see `tir_frame_config`) puts this class's
        // draw near even odds, so a few nonces win; a probe with fabricated roots would be — rightly — pursued by every public
        // bond's fraud filer for its whole liability horizon.
        let backend = self.backend(chain);
        let bond = chain.bonds[card];
        for attempt_index in 0..400 {
            chain.time += chain.config.params.target_time_per_block();
            // A new execution anchor needs a new nonce BUCKET (the anchor reads `nonce >> PALW_TICKET_NONCE_BUCKET_LOG2`).
            chain.nonce += 1u64 << PALW_TICKET_NONCE_BUCKET_LOG2;
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
            let anchor = execution_anchor_v3(chain.network_domain, pre_pow, self.class_id, &bond.0, header.nonce);
            let (job, prompt) = backend.job_for_anchor(anchor).expect("the job this template implies");
            let job = palw_attempt_job_v1(job, chain.config.params.palw_prefill_draw_active_at(header.daa_score));
            let run = backend.execute(&job, &prompt).expect("the honest run");
            let attempt = palw_attempt_from_execution_v1(
                chain.network_domain,
                pre_pow,
                header.timestamp,
                header.nonce,
                bond.0,
                card_pubkey(card),
                &PalwAttemptChainFactsV1 {
                    class_id: self.class_id,
                    artifact_root: facts.artifact_root,
                    pwu: facts.pwu,
                    min_trace_retention_daa: facts.min_trace_retention_daa,
                    witness_chunks: facts.witness_chunks,
                    operator_id: bond_facts.operator_id,
                },
                &PalwAttemptExecutionV1 {
                    trace_root: run.trace_root,
                    output_root: run.output_root,
                    execution_root: run.execution_root,
                    trace_manifest_root: run.trace_manifest_root,
                    trace_chunk_count: run.trace_chunk_count,
                },
                header.daa_score,
            );
            if attempt_index == 0 && chain.daa() < 100 {
                eprintln!("[service-tir-frame] honest probe odds {:.3e} (pwu {})", facts.class_target as f64 / u128::MAX as f64, facts.pwu);
            }
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
            eprintln!("[service-tir-frame] honest IR probe {claim}, DAA {}, accepted {accepted}, lifecycle {:?}", chain.daa(), self.lifecycle(chain));
            return accepted.then_some(claim);
        }
        panic!("the IR class lottery is winnable within 400 honest draws at the harness's W");
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
                let Some(claim) = self.ir_attempt(chain, PROBER).await else {
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
        let read = chain.tc.palw_model_registry_v1().unwrap();
        for c in &read.classes {
            if let Some(row) = c.row.as_ref() {
                eprintln!(
                    "[service-tir-frame] registry class {} base {}: economic ccu/claim {}, verification ccu {}",
                    c.class_id, c.is_base_class, row.work.economic_ccu_per_claim, row.work.verification_ccu
                );
            }
        }
        let floor = read.classes.iter().find(|c| c.is_base_class).unwrap().row.as_ref().unwrap();
        let quantum = kaspa_consensus_core::palw_freeprompt_v3::fp_network_quantum_v1(
            floor.work.economic_ccu_per_claim,
            chain.bundle.state.fp_quanta_per_canonical_job(),
        );
        eprintln!("[service-tir-frame] network FP quantum {quantum} (quanta per canonical job {})", chain.bundle.state.fp_quanta_per_canonical_job());
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
pub(super) async fn register_outsider(chain: &mut Chain, floats: &mut [Funding], fixture: &TirFixture) -> PalwBondKeyV2 {
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
    let until = chain.daa() + maturity + 1;
    // The class's seats stay ready through the wait (an `Active` class whose readiness lapses is `Held`, and `Held` re-enters the
    // probation).
    while chain.daa() < until {
        fixture.refresh_readiness(chain, floats).await;
        let next = (chain.daa() + 12).min(until);
        chain.advance(next).await;
    }
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

/// The node's service stack: the consensus manager and the mempool every service submits into, and the running services — each with
/// its OWN flow context (one gossip inbox per service: a second service on a shared flow disables itself).
pub(super) struct Node {
    pub manager: Arc<ConsensusManager>,
    pub pool: MiningManagerProxy,
    tick: Arc<TickService>,
    pub services: Vec<ServiceRun>,
    flows: Vec<Arc<FlowContext>>,
    /// The socket P2P mesh between the services: the first service's flow listens on loopback, every later one connects to it
    /// (`palw_service_network_e2e`'s transport) — so a seat's material pull, the producer's answer and every seat's receipt
    /// travel as on a real network, relayed by the gossip's own rules.
    adaptors: Vec<Arc<kaspa_p2p_lib::Adaptor>>,
    listen: Option<std::net::SocketAddr>,
    _meta: Vec<kaspa_database::utils::DbLifetime>,
}

impl Node {
    pub fn new(chain: &Chain) -> Self {
        let manager = Arc::new(ConsensusManager::new(Arc::new(TestConsensusFactory::new(chain.tc.clone()))).unwrap());
        let config = Arc::new(chain.config.clone());
        let tick = Arc::new(TickService::new());
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
        let _ = &chain;
        Node { manager, pool, tick, services: Vec::new(), flows: Vec::new(), adaptors: Vec::new(), listen: None, _meta: Vec::new() }
    }

    pub fn start(&mut self, chain: &Chain, config: PalwPanelConfig) {
        let params = Arc::new(chain.config.clone());
        let (meta_lifetime, meta_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let (addresses, _) = kaspa_addressmanager::AddressManager::new(params.clone(), meta_db, self.tick.clone());
        let hub = kaspa_p2p_lib::Hub::new();
        let rules = Arc::new(MiningRuleEngine::new(
            self.manager.clone(),
            params.clone(),
            Default::default(),
            self.tick.clone(),
            hub.clone(),
            Default::default(),
            Arc::new(kaspa_core::chain_participation::ChainParticipationGate::new(true)),
        ));
        let flow = Arc::new(FlowContext::new(
            self.manager.clone(),
            addresses,
            params.clone(),
            self.pool.clone(),
            self.tick.clone(),
            chain.tc.notification_root(),
            hub,
            rules,
        ));
        let service = Arc::new(PalwPanelService::new(config, self.manager.clone(), flow.clone(), params));
        let cloned = service.clone();
        let task = Some(tokio::spawn(async move { cloned.start().await.unwrap() }));
        self.services.push(ServiceRun { service, task });
        self.flows.push(flow);
        self._meta.push(meta_lifetime);
    }

    /// **Connect the started services by actual socket P2P**: the first service's flow listens on a loopback port, every other
    /// flow dials it (the handshake and the gossip flows are production's).
    pub async fn connect(&mut self) {
        let address = match self.listen {
            Some(address) => address,
            None => {
                let first = self.flows.first().expect("a started service");
                let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
                let address = listener.local_addr().unwrap();
                drop(listener);
                let primary =
                    kaspa_p2p_lib::Adaptor::bidirectional(address.into(), first.hub().clone(), first.clone(), Default::default()).unwrap();
                self.adaptors.push(primary);
                self.listen = Some(address);
                address
            }
        };
        // Every flow past the listener's and past those already dialled (each adaptor after the first is one dialled flow).
        let dialled = self.adaptors.len();
        for flow in self.flows[dialled..].to_vec() {
            let client = kaspa_p2p_lib::Adaptor::client_only(flow.hub().clone(), flow.clone(), Default::default());
            tokio::time::timeout(Duration::from_secs(15), async {
                loop {
                    match client.connect_peer(address.to_string()).await {
                        Ok(peer) => break peer,
                        Err(_) => tokio::time::sleep(Duration::from_millis(25)).await,
                    }
                }
            })
            .await
            .expect("the loopback handshake completes");
            self.adaptors.push(client);
        }
        eprintln!("[service-tir-frame] {} services connected by socket P2P through {address}", self.flows.len());
    }

    /// One block of the production template over the mempool the services feed (two when carriers were pending), its clock held
    /// near the wall clock.
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
        for adaptor in self.adaptors.drain(..) {
            adaptor.close().await;
        }
    }
}

/// One node step, the harness re-proving the seats' readiness every `READINESS_EVERY` DAA (see `tn1_node`).
async fn step_ready(node: &Node, chain: &mut Chain, floats: &mut [Funding], fixture: &TirFixture, last: &mut u64) {
    const READINESS_EVERY: u64 = 12;
    if chain.daa() >= *last + READINESS_EVERY {
        fixture.refresh_readiness(chain, floats).await;
        *last = chain.daa();
    }
    node.step(chain).await;
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

/// Heartbeats carry transactions and advance DAA, but never supply a receipt
/// beacon. Add an actual executed attempt for each newly matured set of slots.
/// The attempt's own class lottery is solved normally; no receipt ticket is
/// inspected while selecting the block, and a slot is never rerolled on a loss.
async fn supply_receipt_beacon(
    chain: &mut Chain,
    fixture: &TirFixture,
    supplied: &mut BTreeSet<u64>,
) {
    let candidates = framed_claims(chain);
    let mut due = BTreeSet::new();
    chain.tc.palw_tir_public_read_v1(&mut |state| {
        for candidate in &candidates {
            let Some(claim) = state.claim(candidate) else { continue };
            let PalwClaimPhaseV2::Final { final_daa } = claim.phase else { continue };
            let Ok(clock) = kaspa_consensus_core::palw_rfc0018_v1::palw_fp_receipt_clock_v2(
                &chain.bundle.state, state, candidate, claim, final_daa,
            ) else { continue };
            let Some(slot) = kaspa_consensus_core::palw_freeprompt_v3::fp_draw_slot_v3(
                clock, chain.bundle.freeprompt.receipt_maturity_daa(),
            ) else { continue };
            if slot <= chain.daa() && !supplied.contains(&slot) {
                due.insert(slot);
            }
        }
        Ok(())
    }).expect("receipt clocks read accepted claim state");
    if !due.is_empty() && let Some(attempt) = fixture.ir_attempt(chain, PROBER).await {
        eprintln!("[service-tir-frame] honest attempt {attempt} supplies receipt beacon slots {due:?} at DAA {}", chain.daa());
        supplied.extend(due);
    }
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
    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        chain.config.params.net.to_string().as_bytes(), Some(chain.config.params.genesis.hash),
    );
    let claim_id = win.claim_id;
    let quantum_index = win.quantum_index;
    let beacon_block = win.beacon.beacon_block;
    let challenge = kaspa_consensus_core::palw_freeprompt_v3::spend_challenge_v3(
        network_domain, pre_pow, template.block.header.timestamp, template.block.header.nonce,
        claim_id, quantum_index, &bond,
    );
    let spend = kaspa_consensus_core::palw_freeprompt_v3::PalwReceiptSpendUnsignedV3 {
        version: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V3_VERSION,
        network_domain, challenge, claim_id, quantum_index, beacon_block, producer_bond: bond,
        producer_pubkey: card_pubkey(card),
    };
    let signature = signed(
        card, kaspa_consensus_core::palw_freeprompt_v3::fp_spend_id_v3(&spend).as_byte_slice(),
        kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V3_MLDSA87_SPEND_CONTEXT,
    );
    template.block.header.palw_commitment = borsh::to_vec(
        &kaspa_consensus_core::palw_freeprompt_v3::PalwReceiptSpendEnvelopeV3 { spend, signature },
    ).unwrap();
    template.block.header.finalize();
    let block = chain.insert(template.block).await;
    Some(block.hash())
}

/// **The colluding Panel's own objects for a forged framed claim** (the adversary is harness-scripted; honest software would refuse to
/// sign): each bound seat files ONE preparation debit for its pass and its atomic Valid + raw custody grant + Ready, then the Panel
/// licenses on exactly those credited receipts — every object through the PQ mempool and the production template.
async fn colluders_prepare(chain: &mut Chain, floats: &mut [Funding], claim: Hash64) -> Vec<usize> {
    use kaspa_consensus_core::palw_tir_public_carriage_v1::{
        PALW_TIR_PUBLIC_CONTROL_CONTEXT_V1, PalwTirPublicControlActionV1 as A, PalwTirPublicControlTargetV1 as T, PalwTirPublicControlV1,
    };
    use kaspa_consensus_core::palw_tir_public_owned_v1::{PalwTirPublicWorkPhaseV1, PalwTirPublicWorkScopeV1};
    // 1. Each drawn seat's one pass debit, prepared on the accepted state through the consensus work book — inside the frame's
    //    preparation window, which closes long before the Panel's bind (the seats are the frame's drawn Panel).
    let mut debits = Vec::new();
    let mut frame_out = None;
    let mut seats: Vec<usize> = Vec::new();
    chain
        .tc
        .palw_tir_public_read_v1(&mut |state| {
            let frame = kaspa_consensus_core::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim).ok_or("no frame")?.clone();
            let plan = frame.plan_v1().clone();
            let key = kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedCourtKeyV1::TirPreparation(claim, plan.allocation_id);
            let maximum = frame.descriptor_v1().maximum_leaves;
            let snapshot =
                kaspa_consensus_core::palw_tir_public_state_v1::palw_tir_public_preparation_snapshot_v1(state, key, maximum)?.ok_or("no row")?;
            let contract = snapshot.row.preparation.contract_v1();
            eprintln!(
                "[service-tir-frame] forged frame: preparation until DAA {} (now {}), drawn Panel of {}",
                contract.preparation_until_daa,
                snapshot.now_daa,
                contract.drawn_panel.len()
            );
            seats = contract.drawn_panel.iter().filter_map(|bond| chain.bonds.iter().position(|b| b == bond)).collect();
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
    let mut controls = Vec::new();
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
        controls.push((card, PalwConsensusObjectV2::TirPublicControlV1 { control: Box::new(control) }));
    }
    send_batch(chain, floats, controls).await;
    seats
}

/// **The colluding Panel's licence** (after the bind): each bound seat's atomic Valid + raw custody grant + Ready, then the licence on
/// exactly those credited receipts — through the PQ mempool and the production template.
async fn colluders_license(
    node: &Node,
    chain: &mut Chain,
    floats: &mut [Funding],
    fixture: &TirFixture,
    ready_at: &mut u64,
    claim: Hash64,
    drawn: Vec<usize>,
) -> Vec<usize> {
    use kaspa_consensus_core::palw_tir_public_carriage_v1::{PALW_TIR_PUBLIC_GRANT_READY_CONTEXT_V1, PalwTirPublicGrantReadyV1};
    let duties = chain.tc.palw_seat_duties_v2(chain.bonds.clone()).into_iter().filter(|d| d.claim_id == claim).collect::<Vec<_>>();
    assert!(!duties.is_empty(), "the forged claim's Panel is bound: {:?}", chain.view(claim));
    let seats: Vec<usize> = duties.iter().map(|d| chain.bonds.iter().position(|b| *b == d.seat_bond).unwrap()).collect();
    // A Panel the frame drew at acceptance prepared above; a Panel bound only later (the node's anchor delay) has no preparation lane.
    assert!(drawn.iter().all(|card| seats.contains(card)), "a drawn seat is a bound seat: drawn {drawn:?}, bound {seats:?}");
    if !drawn.is_empty() {
        assert_eq!(
            drawn.iter().copied().collect::<BTreeSet<_>>(),
            seats.iter().copied().collect::<BTreeSet<_>>(),
            "all bound colluders must prepare and sign before conviction"
        );
    }
    let mut frame_out = None;
    chain
        .tc
        .palw_tir_public_read_v1(&mut |state| {
            frame_out = kaspa_consensus_core::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim).cloned();
            Ok(())
        })
        .expect("the accepted TIR state reads");
    let frame = frame_out.expect("the frame");
    let plan = frame.plan_v1().clone();
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
    let mut grants = Vec::new();
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
        // A Panel bound after the frame's preparation window has no Ready lane (the carrier is refused as missing the fixed
        // preparation deadline); such a Panel licenses on its plain Valid receipts, as the honest seats' services do.
        if !drawn.is_empty() {
            grants.push((*card, PalwConsensusObjectV2::TirPublicGrantReadyV1 { grant: Box::new(movement) }));
        }
        receipts.push(receipt);
    }
    send_batch(chain, floats, grants).await;
    // 3. The licence on exactly those receipts. A Panel with Ready lanes licenses by the V2 quorum on its credited receipts; a Panel
    //    bound after the preparation window licenses as the honest services do past R-core+ — by coverage, on V3 receipts each over
    //    the seat's assigned segment mask.
    let object = if drawn.is_empty() {
        let mut v3 = Vec::new();
        for duty in &duties {
            let card = chain.bonds.iter().position(|b| *b == duty.seat_bond).unwrap();
            let assignment = kaspa_consensus_core::palw_verification_v2::palw_segment_assignment_v2(
                duty.panel_anchor,
                duty.claim_id,
                duty.panel_seat_count.max(1),
            );
            let mask = assignment.mask_of(duty.seat_index as u16);
            let signed_daa = chain.daa();
            let message = kaspa_consensus_core::palw_panel_v2::palw_receipt_message_v3(
                chain.network_domain,
                claim,
                PalwReceiptVerdictV2::Valid,
                signed_daa,
                mask,
            );
            let inner = PalwSeatReceiptV2 {
                claim,
                verdict: PalwReceiptVerdictV2::Valid,
                seat_bond: duty.seat_bond,
                signed_daa,
                signature: signed(card, message.as_byte_slice(), kaspa_consensus_core::palw_panel_v2::PALW_RECEIPT_V3_MLDSA87_CONTEXT),
            };
            v3.push(kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV3 { receipt: inner, segments: mask });
        }
        // The coverage door opens only once the chain's receipt clock lets it (the honest services pool and offer until it does):
        // offer each block — the node's blocks (the services' carriers ride them), paced as the main loop paces the frame's door.
        let mut licence = None;
        for _ in 0..1_500 {
            if chain.view(claim).phase.is_terminal() {
                break;
            }
            if let Some(object) = chain
                .tc
                .palw_v2_receipt_coverage_assemble(claim, v3.clone())
                .or_else(|| chain.tc.palw_v2_receipt_quorum_assemble(claim, receipts.clone()))
                .or_else(|| chain.tc.palw_v2_optimistic_assemble(claim, v3.clone()))
            {
                licence = Some(object);
                break;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
            step_ready(node, chain, floats, fixture, ready_at).await;
        }
        eprintln!("[service-tir-frame] colluding licence offer ends at DAA {} (assembled: {}): {:?}", chain.daa(), licence.is_some(), chain.view(claim).phase);
        licence
    } else {
        Some(chain.tc.palw_v2_receipt_quorum_assemble(claim, receipts).expect("the colluding Panel licenses"))
    };
    // Convicted before the colluders' licence could land: no Valid receipt is on chain, so no colluding seat is chargeable.
    let Some(object) = object else { return Vec::new() };
    chain.send(seats[0], &object, &mut floats[seats[0]], &[]).await;
    assert!(matches!(chain.view(claim).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "{:?}", chain.view(claim).phase);
    seats
}

/// **T-N1 at node level, both cases.** `forged`: the producer's service commits a cone lie (drill D-05), its whole Panel colludes,
/// and one public bond outside the Panel — the actual service — localizes and convicts.
async fn tn1_node(forged: bool, declared: bool) {
    assert!(!forged || !declared, "the separate forged fixture uses the original semantic identities");
    kaspa_core::log::try_init_logger(&std::env::var("TN1_LOG").unwrap_or_else(|_| "warn,kaspad_lib::palw_panel=info".into()));
    let (mut config, mut bundle, premine, mut floats) = tir_frame_config();
    if forged {
        // HARNESS ONLY: bind the colluding panel before the short preparation door closes.
        // The external service still discovers, acquires and convicts the claim itself.
        bundle.panel =
            kaspa_consensus_core::palw_panel_v2::PalwPanelParamsV2::new(bundle.panel.seat_count(), bundle.panel.quorum(), 1).unwrap();
        let mut params = config.params.clone();
        params.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
        config = Config::new(params);
    }
    if declared {
        let mut params = config.params.clone();
        params.palw_class_semantics_v1 = Some(ForkActivation::new(0));
        config = Config::new(params);
    }
    let mut chain = Chain::new(config, bundle, &premine);
    chain.advance(2).await;
    let fixture = TirFixture::new_with_semantics(&chain, declared);
    fixture.onboard_to_active(&mut chain, &mut floats).await;
    if declared {
        fixture.declare_semantics(&mut chain, &mut floats).await;
    }
    let outsider = register_outsider(&mut chain, &mut floats, &fixture).await;
    // The producer's frame rent (the exact native rent a TFP1 original declares and pays on top of its relay fee, at the TEST-ONLY
    // unit prices over the frame's door) is far past a card's fee float: the outside verifier's main-premine float funds the
    // producer's service float, on an idempotent public carrier (the already-published layout, which folds to nothing).
    let producer_float = {
        let republish =
            PalwConsensusObjectV2::TirClassLayoutPublishedV1 { class_id: fixture.class_id, layout: Box::new(fixture.layout.clone()) };
        let amount = floats[OUTSIDER].1.amount / 4;
        let tx = chain.send(OUTSIDER, &republish, &mut floats[OUTSIDER], &[TransactionOutput::new(amount, card_payout_spk(0))]).await;
        eprintln!("[service-tir-frame] producer service float {amount} sompi (from the outsider's premine float)");
        (TransactionOutpoint::new(tx.id(), 0), UtxoEntry::new(tx.outputs[0].value, tx.outputs[0].script_public_key.clone(), chain.daa(), false))
    };
    // The outside verifier's actual service runs from here on (its fraud filer works through the harness's ground probe claims —
    // fabricated roots, rightly pursued — while the chain walks to the frame route, not later inside a frame's owner window).
    let dirs = tempfile::tempdir().unwrap();
    let mut node = Node::new(&chain);
    let config = service_config(&chain, &dirs.path().join("outsider"), OUTSIDER, outsider, &floats[OUTSIDER], &fixture);
    node.start(&chain, config);
    node.connect().await;
    // The frame route is in force only where every rule its verdicts consume is (testnet-12's aggregate liability at DAA 1,700 is the
    // latest): walk there through the node (the outsider's carriers ride the template), the seats' readiness kept fresh.
    let route = chain.config.params.palw_tir_public_conviction_v1_fence().expect("the frame route is armed").daa_score();
    let mut ready_at = chain.daa();
    while chain.daa() < route + 2 {
        step_ready(&node, &mut chain, &mut floats, &fixture, &mut ready_at).await;
    }
    let class_id = fixture.class_id;
    let mut layout = None;
    assert_eq!(
        chain.tc.palw_tir_public_read_v1(&mut |state| {
            layout = kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state, class_id).cloned();
            Ok(())
        }),
        Ok(true),
        "the frame route is in force"
    );
    assert_eq!(layout.as_ref(), Some(&fixture.layout), "piece 2: the published layout is on chain");
    fixture.refresh_readiness(&mut chain, &mut floats).await;
    assert_eq!(fixture.lifecycle(&chain), Some(PalwModelLifecycleV1::Active), "the IR class stays Active");
    // The harness keeps the seats' readiness fresh while the services run: testnet-12's heartbeat clock paces one DAA per 120 s of
    // block time, so a chain that moves faster than the wall clock is never `is_nearly_synced` and a service files no possession
    // proof of its own (every other duty runs). So each card's fee float is split first — one output for its service, one for the
    // harness — and neither ever spends the other's.
    let mut service_floats = floats.clone();
    for card in 0..chain.bonds.len() {
        let half = floats[card].1.amount / 2;
        let object = if card == 0 {
            let capable_classes = BTreeSet::from([chain.bundle.base_class_id, fixture.class_id]);
            let message =
                kaspa_consensus_core::palw_state_v2::palw_bond_capability_message_v2(chain.network_domain, &chain.bonds[0], &capable_classes);
            PalwConsensusObjectV2::BondCapabilityDeclared {
                bond: chain.bonds[0],
                capable_classes,
                signature: signed(0, message.as_byte_slice(), kaspa_consensus_core::palw_state_v2::PALW_BOND_CAPABILITY_V2_MLDSA87_CONTEXT),
            }
        } else {
            fixture.readiness(&chain, card)
        };
        let tx = chain.send(card, &object, &mut floats[card], &[TransactionOutput::new(half, card_payout_spk(card))]).await;
        service_floats[card] = (
            TransactionOutpoint::new(tx.id(), 0),
            UtxoEntry::new(tx.outputs[0].value, tx.outputs[0].script_public_key.clone(), chain.daa(), false),
        );
    }
    service_floats[0] = producer_float;
    // Piece 3: the producer's actual service — canonical free-prompt claims on the IR class, published as ORIGINAL frames (`TFP1`).
    let mut producer = service_config(&chain, &dirs.path().join("producer"), 0, chain.bonds[0], &service_floats[0], &fixture);
    producer.canonical_claims = true;
    producer.canonical_class = Some(fixture.class_id.to_string());
    producer.canonical_interval_daa = 200;
    if forged {
        producer.drill_tamper_tir_frame = Some(super::super::PalwDrillTirFrameFaultV1::Cone { leaf: 5 });
    }
    node.start(&chain, producer);
    // Piece 4: the honest Panel's seats are actual services (they replay, sign, prepare, grant + Ready and license); in the forged
    // case the whole Panel colludes (harness-scripted below), so no seat service runs. The outside verifier always runs.
    if !forged {
        for card in 1..chain.bonds.len() {
            let config =
                service_config(&chain, &dirs.path().join(format!("seat{card}")), card, chain.bonds[card], &service_floats[card], &fixture);
            node.start(&chain, config);
        }
    }
    node.connect().await;
    let mut ready_at = chain.daa();
    let deadline = std::time::Instant::now() + Duration::from_secs(7_200);
    // The producer's first framed claim.
    let claim = loop {
        assert!(std::time::Instant::now() < deadline, "the producer service carried no TFP1 claim");
        step_ready(&node, &mut chain, &mut floats, &fixture, &mut ready_at).await;
        if let Some(claim) = framed_claims(&chain).into_iter().find(|c| chain.tc.palw_tir_public_frame_v1(*c).ok().flatten().is_some()) {
            break claim;
        }
    };
    let frame = chain.tc.palw_tir_public_frame_v1(claim).unwrap().unwrap();
    eprintln!("[service-tir-frame] piece 3: framed claim {claim} accepted at DAA {}: {frame:?}", chain.daa());
    if declared {
        let context = &frame.header.job_context;
        assert_eq!(context.model_profile_id, Hash64::from_u64_word(0x02B1));
        assert_eq!(context.runtime_manifest_hash, Hash64::from_u64_word(0x02B2));
        assert_eq!(context.runtime_class_id, Hash64::from_u64_word(0x02B3));
        assert_eq!(context.cu_ruleset_id, Hash64::from_u64_word(0x02B4));
        let mut original = None;
        chain
            .tc
            .palw_tir_public_read_v1(&mut |state| {
                let semantics = state.class_semantics_v1(&fixture.class_id).expect("the signed semantic declaration");
                assert!(semantics.matches_context(context));
                assert_eq!(
                    state.staged_tokenizer_inventory_v1(semantics.tokenizer_id).unwrap().actual_bytes,
                    fixture.semantic_source.as_ref().unwrap().0.len() as u64
                );
                original = kaspa_consensus_core::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim)
                    .map(|frame| frame.original_v1().clone());
                Ok(())
            })
            .unwrap();
        let original = original.unwrap();
        assert!(chain.tc.palw_tir_public_original_quote_v1(original.clone(), frame.header.clone()).unwrap().is_some());
        let mut wrong = frame.header.clone();
        wrong.job_context.runtime_manifest_hash = Hash64::from_u64_word(0xBAD02B);
        assert!(
            chain.tc.palw_tir_public_original_quote_v1(original, wrong).is_err(),
            "02-B: changing only the registered manifest must refuse the frame"
        );
    }
    // The colluding Panel prepares inside the frame's preparation window (harness-scripted adversary).
    let producer_bond = chain.bonds[0];
    let collateral_of = |chain: &Chain, bond: PalwBondKeyV2| {
        chain
            .tc
            .palw_claim_rows_v1(bond, kaspa_consensus_core::palw_producer_v2::PalwClaimRoleV1::Executor, false, 0)
            .unwrap()
            .bond
            .unwrap()
            .collateral
    };
    let before = collateral_of(&chain, producer_bond);
    let mut colluders = Vec::new();
    let mut colluder_collateral = std::collections::BTreeMap::new();
    if forged {
        let slot = chain.view(claim).accepted_daa + chain.bundle.panel.anchor_delay();
        while chain.daa() < slot {
            step_ready(&node, &mut chain, &mut floats, &fixture, &mut ready_at).await;
        }
        anchor_block(&mut chain).await;
        let drawn = colluders_prepare(&mut chain, &mut floats, claim).await;
        assert!(!drawn.is_empty(), "the early bound panel must prepare and sign");
        colluder_collateral = drawn.iter().map(|c| (*c, collateral_of(&chain, chain.bonds[*c]))).collect();
        colluders = colluders_license(&node, &mut chain, &mut floats, &fixture, &mut ready_at, claim, drawn).await;
        assert!(!colluders.is_empty(), "a forged claim must be licensed before conviction to exercise seat slash");
    }
    // **The harness's clock against the services' wall clock**: a block here takes ~60 ms, a service's replay and open take seconds,
    // and the frame's owner-open door is a few DAA under the TEST-ONLY clock (a real network's DAA is a heartbeat). Through the
    // open door the harness paces one DAA per 4 s, so the outside service's open lands inside it as it would on a network.
    let (mut open_until, mut collect_until) = (0, 0);
    chain
        .tc
        .palw_tir_public_read_v1(&mut |state| {
            if let Some(frame) = kaspa_consensus_core::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim) {
                open_until = frame.plan_v1().terms.open_until_daa;
                collect_until = frame.plan_v1().terms.collect_until_daa;
            }
            Ok(())
        })
        .unwrap();
    eprintln!("[service-tir-frame] owner-open door until DAA {open_until} (now {})", chain.daa());
    let mut preparation_until = 0;
    chain
        .tc
        .palw_tir_public_read_v1(&mut |state| {
            preparation_until = kaspa_consensus_core::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim)
                .map(|frame| frame.plan_v1().preparation_until_daa)
                .unwrap_or(0);
            Ok(())
        })
        .unwrap();
    while chain.daa() <= open_until {
        // An owner may pursue only from the preparation door on (`owner_may_pursue_v1`): the slow pace covers the owner's window.
        let pace = if chain.daa() + 1 >= preparation_until { 90 } else { 4 };
        tokio::time::sleep(Duration::from_secs(pace)).await;
        step_ready(&node, &mut chain, &mut floats, &fixture, &mut ready_at).await;
    }
    // The honest Panel is drawn at the anchor after the claim's anchor delay.
    if !forged {
        let slot = chain.view(claim).accepted_daa + chain.bundle.panel.anchor_delay();
        while chain.daa() + 2 < slot {
            step_ready(&node, &mut chain, &mut floats, &fixture, &mut ready_at).await;
        }
        // Fresh readiness right before the draw (the harness's, see above).
        fixture.refresh_readiness(&mut chain, &mut floats).await;
        ready_at = chain.daa();
        while chain.daa() < slot {
            step_ready(&node, &mut chain, &mut floats, &fixture, &mut ready_at).await;
        }
        anchor_block(&mut chain).await;
    }
    loop {
        assert!(std::time::Instant::now() < deadline, "no outcome: {:?}", chain.view(claim));
        // The owner's acquisition, dissection and filing run on its own loop (tens of moves, seconds each in this debug build): through
        // the frame's collection door the harness paces one DAA per 2 s (a testnet-12 DAA is 120 s).
        if forged && chain.daa() <= collect_until {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        step_ready(&node, &mut chain, &mut floats, &fixture, &mut ready_at).await;
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
        for card in &colluders {
            assert!(collateral_of(&chain, chain.bonds[*card]) < colluder_collateral[card], "colluding seat {card} is slashed");
        }
        for _ in 0..20 {
            step_ready(&node, &mut chain, &mut floats, &fixture, &mut ready_at).await;
        }
        assert!(spend_one_quantum(&mut chain, 0, claim).await.is_none(), "a voided claim's quanta never spend");
        eprintln!("[service-tir-frame] forged: convicted by the outside service at DAA {}", chain.daa());
    } else {
        assert_eq!(collateral_of(&chain, producer_bond), before, "the honest producer is not charged");
        // Require a real receipt spend. The producer keeps submitting honest frames: if this
        // frame's fixed beacon loses, observe subsequent Final frames rather than treating a
        // payment right as payment or rerolling the same claim's ticket.
        let mut source = None;
        chain
            .tc
            .palw_tir_public_read_v1(&mut |state| {
                source = state.claim(&claim).map(|c| (c.source.clone(), c.pwu));
                Ok(())
            })
            .unwrap();
        let (source, pwu) = source.expect("the Final claim is in the state");
        let kaspa_consensus_core::palw_state_v2::PalwClaimSourceV2::FreePrompt { quanta, spent } = &source else { panic!("a free-prompt claim: {source:?}") };
        assert!(*quanta >= 1 && spent.is_empty() && pwu > 0, "the Final claim holds unspent priced quanta: {source:?} pwu {pwu}");
        let mut paid = None;
        let mut beacon_slots = BTreeSet::new();
        for _ in 0..1_800 {
            assert!(std::time::Instant::now() < deadline, "no honest framed claim paid before the test deadline");
            supply_receipt_beacon(&mut chain, &fixture, &mut beacon_slots).await;
            let (_, state) = chain.tc.palw_state_v2_store.read().load_tip(&chain.bundle.state).unwrap().unwrap();
            let mut finalized = 0usize;
            let mut mature = 0usize;
            for candidate in framed_claims(&chain) {
                let Some(row) = state.claim(&candidate) else { continue };
                let PalwClaimPhaseV2::Final { final_daa } = row.phase else { continue };
                finalized += 1;
                if kaspa_consensus_core::palw_rfc0018_v1::palw_fp_receipt_clock_v2(
                    &chain.bundle.state, &state, &candidate, row, final_daa,
                )
                .ok()
                .and_then(|clock| {
                    kaspa_consensus_core::palw_freeprompt_v3::fp_draw_slot_v3(
                        clock,
                        chain.bundle.freeprompt.receipt_maturity_daa(),
                    )
                })
                .is_some_and(|slot| slot <= chain.daa())
                {
                    mature += 1;
                }
            }
            for candidate in framed_claims(&chain) {
                if matches!(chain.view(candidate).phase, PalwClaimPhaseV2::Final { .. })
                    && let Some(block) = spend_one_quantum(&mut chain, 0, candidate).await
                {
                    paid = Some((candidate, block));
                    break;
                }
            }
            if paid.is_some() {
                break;
            }
            if chain.daa() % 50 == 0 {
                eprintln!(
                    "[service-tir-frame] receipt progress at DAA {}: finalized frames {finalized}, mature draw slots {mature}, attempt beacons supplied {}, spendable winners none yet",
                    chain.daa(),
                    beacon_slots.len()
                );
            }
            step_ready(&node, &mut chain, &mut floats, &fixture, &mut ready_at).await;
        }
        let (paid_claim, block) = paid.expect("T-N1 requires an actual quantum spend, not only a payment right");
        let paid_frame = chain.tc.palw_tir_public_frame_v1(paid_claim).unwrap().expect("the paid claim has its ORIGINAL frame");
        if declared {
            assert_eq!(paid_frame.header.job_context.runtime_manifest_hash, Hash64::from_u64_word(0x02B2));
            assert_eq!(paid_frame.header.job_context.cu_ruleset_id, Hash64::from_u64_word(0x02B4));
        }
        let mut spent = false;
        chain
            .tc
            .palw_tir_public_read_v1(&mut |state| {
                spent = matches!(state.claim(&paid_claim).map(|c| &c.source),
                Some(kaspa_consensus_core::palw_state_v2::PalwClaimSourceV2::FreePrompt { spent, .. }) if !spent.is_empty());
                Ok(())
            })
            .unwrap();
        assert!(spent, "the receipt block must consume the claim's quantum in consensus state");
        assert_eq!(collateral_of(&chain, producer_bond), before, "payment never charges the honest producer");
        eprintln!("[service-tir-frame] honest: claim {paid_claim} Final and paid in receipt block {block} at DAA {}", chain.daa());
    }
    node.finish().await;
}

/// **T-N1, honest, at node level**: the producer service's framed claim is replayed (material pulled over socket P2P) and licensed
/// by the seats' actual services and reaches `Final`. At least one honest framed claim's
/// quantum must be consumed by an actual receipt block; the producer remains uncharged.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tir_frame_node_tn1_an_honest_framed_claim_is_final_with_its_payment_right() {
    on_service_stack(|| tn1_node(false, true)).await;
}

/// **T-N1, forged, at node level**: the producer service commits a cone lie, the whole Panel colludes, and the outside public bond's
/// actual service convicts it — voided `CourtFraud`, producer and Valid seats slashed, never paid.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tir_frame_node_tn1_a_forged_framed_claim_is_convicted_by_one_outside_service() {
    on_service_stack(|| tn1_node(true, false)).await;
}


/// Isolate the ordinary T-N1 payment requirement from the additional 02-B semantics
/// acceptance case above. Both cases remain required; a pass here does not close 02-B.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tir_frame_node_tn1_default_semantics_spends_an_actual_honest_quantum() {
    on_service_stack(|| tn1_node(false, false)).await;
}
