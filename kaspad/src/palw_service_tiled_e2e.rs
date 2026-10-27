//! A normal bought A16 class, held from an independent file by the outside verifier.
//! Small geometry is deliberate; no profile, price, admission or conviction callback is replaced.
use super::*;
use kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1;
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};

pub(super) struct TiledFixture {
    _dir: tempfile::TempDir,
    pub path: std::path::PathBuf,
    pub profile: PalwShapeProfileV3,
    pub canonical: PalwJobContextV2,
    pub root: Hash64,
    pub table: Arc<kaspa_consensus_core::palw_fp_constraint_job_v1::PalwTokenTableV1>,
}

impl TiledFixture {
    pub fn new(chain: &Chain) -> Self {
        Self::new_with_tokenizer_padding(chain, 0)
    }

    pub(super) fn new_with_tokenizer_padding(chain: &Chain, padding: usize) -> Self {
        use kaspa_consensus_core::palw_qwen25_profile::*;
        use kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES;
        let read = chain.tc.palw_model_registry_v1().unwrap();
        let floor = read.classes.iter().find(|c| c.is_base_class).unwrap().row.as_ref().unwrap();
        let quantum = kaspa_consensus_core::palw_freeprompt_v3::fp_network_quantum_v1(
            floor.work.economic_ccu_per_claim,
            chain.bundle.state.fp_quanta_per_canonical_job(),
        );
        assert!(quantum > 0);
        // Choose an ordinary admissible geometry whose real seven-position work reaches
        // the network's existing quantum. Neither the divisor nor credited work is changed.
        let (geometry, profile) = [16, 32, 64, 128, 256]
            .into_iter()
            .find_map(|hidden| {
                let geometry = PalwQwen25GeometryV1 {
                    layer_count: 2,
                    hidden_dim: hidden,
                    ffn_dim: hidden,
                    attn_heads: 4,
                    attn_kv_heads: 2,
                    attn_head_dim: hidden / 4,
                    vocab_size: PALW_LOGITS_TILE_LANES as u32 + 3,
                    n_ctx: 16,
                    n_threads: 1,
                    rms_eps_q: 1,
                    tile_len: 128,
                };
                let profile = qwen25_a16_profile_v2(geometry).unwrap();
                let credited = kaspa_consensus_core::palw_freeprompt_v3::fp_derive_credited_compute_v1(&profile, 3, 4, 0).unwrap();
                (credited >= quantum * 2).then(|| {
                    eprintln!(
                        "[service-tiled] ordinary geometry hidden {hidden}, FP work {credited}, native network quantum {quantum}"
                    );
                    (geometry, profile)
                })
            })
            .expect("an ordinary A16 geometry reaches the native FP quantum");
        let canonical = kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_canonical_v1(&profile, false).unwrap();
        let mut canonical = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, canonical.0, canonical.1);
        let shape = Base0ShapeV1 {
            n_layers: 2,
            n_heads: 4,
            n_kv_heads: 2,
            d_head: geometry.attn_head_dim as usize,
            d_ff: geometry.ffn_dim as usize,
            vocab: geometry.vocab_size as usize,
            max_position: geometry.n_ctx as usize,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1,
        };
        // Derive the public rendering table from the same actual tokenizer bytes whose
        // commitment the independently loaded artifact publishes. No zero-tokenizer waiver.
        let vocab: serde_json::Map<String, serde_json::Value> = (0..shape.vocab)
            .map(|id| {
                (
                    match id {
                        0 => "a".into(),
                        PALW_LOGITS_TILE_LANES => "aa".into(),
                        _ => format!("b{id}"),
                    },
                    id.into(),
                )
            })
            .collect();
        let mut tokenizer_json = serde_json::json!({"model":{"type":"BPE","vocab":vocab,"merges":[]}});
        if padding != 0 {
            tokenizer_json.as_object_mut().unwrap().insert("padding".into(), serde_json::Value::String("x".repeat(padding)));
        }
        let tokenizer_bytes = serde_json::to_vec(&tokenizer_json).unwrap();
        let tokenizer = misaka_palw_base0::tokenizer::QwenTokenizer::from_json(&tokenizer_bytes).unwrap();
        let tokenizer_id = Base0ArtifactV1::tokenizer_commitment_of(&tokenizer_bytes);
        let artifact = Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .unwrap()
            .with_a16_params(misaka_palw_base0::engine_a16::derived_a16_store(&shape))
            .unwrap()
            .with_tokenizer_commitment(tokenizer_id);
        let root = misaka_palw_base0::inventory::a16_inventory_v1(&artifact, &profile).unwrap().root();
        // Register the same five semantic identities the actual held backend executes.
        // The inventory root covers operands, so a zero-tokenizer canonical context cannot
        // stand in for the artifact's independently committed tokenizer/runtime identity.
        let native = misaka_palw_base0::qwen25_a16_backend::Qwen25A16Backend::from_registered_profile_in_lane_v1(
            Arc::new(artifact.clone()),
            chain.config.params.net.to_string().into_bytes(),
            profile.clone(),
            (canonical.declared_prefill_tokens, canonical.exact_decode_tokens),
            misaka_palw_base0::classes::ArtifactSourceV1::ConvertedA16,
        )
        .unwrap();
        let (native_context, _) = native.job_for_anchor(Hash64::default()).unwrap();
        canonical.model_profile_id = native_context.model_profile_id;
        canonical.runtime_manifest_hash = native_context.runtime_manifest_hash;
        canonical.runtime_class_id = native_context.runtime_class_id;
        canonical.tokenizer_id = native_context.tokenizer_id;
        canonical.cu_ruleset_id = native_context.cu_ruleset_id;
        assert_eq!(canonical.tokenizer_id, tokenizer_id);
        let table = Arc::new(kaspa_consensus_core::palw_fp_constraint_job_v1::PalwTokenTableV1 {
            entries: (0..shape.vocab as u32).map(|id| tokenizer.token_bytes(id)).collect(),
            eog_token_ids: vec![shape.vocab as u32 - 1],
            tokenizer_id,
        });
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("outside-model.palwart");
        std::fs::write(&path, misaka_palw_base0::artifact::encode_artifact_file_v1(&artifact)).unwrap();
        std::fs::write(dir.path().join("tokenizer.json"), tokenizer_bytes).unwrap();
        std::fs::write(
            crate::palw_backends::token_table_sidecar_path_v1(&path),
            kaspa_consensus_core::palw_fp_constraint_job_v1::palw_token_table_file_encode_v1(&table),
        )
        .unwrap();
        Self { _dir: dir, path, profile, canonical, root, table }
    }

    pub fn backend(&self, chain: &Chain) -> Box<dyn PalwExecutionBackendV1> {
        let sdk = misaka_palw_sdk::PalwClassSdk::builtin_v1(
            chain.bundle.court,
            chain.config.params.palw_prompt_ids_form_v1(),
            chain.config.params.net.to_string().into_bytes(),
        )
        .with_chain_classes_v1();
        let holding = sdk.load_artifact(&self.path).unwrap();
        let registry = crate::palw_backends::PalwBackendRegistry::for_node_v1(
            &chain.config.params,
            chain.bundle.court,
            chain.config.params.palw_prompt_ids_form_v1(),
            vec![holding],
            chain.config.params.net.to_string().into_bytes(),
            true,
        );
        registry
            .resolve_or_chain(self.profile.shape_profile_id(), self.root, |id| chain.tc.palw_registered_class_carriage_v1(id))
            .unwrap()
    }

    pub async fn register(&self, chain: &mut Chain, floats: &mut [Funding]) {
        let terms = chain.tc.palw_v2_registration_terms().unwrap();
        let mut object = kaspa_consensus_core::palw_class_admission_v2::palw_post_genesis_registration_capped_v1(
            self.profile.clone(),
            self.canonical.clone(),
            self.root,
            terms.min_grantable_share_permille,
            terms.initial_target,
            terms.slash_value_per_pwu,
            0,
            chain.bonds[0],
            vec![],
            chain.bundle.court.max_step_leaf_count(),
        )
        .unwrap();
        let PalwConsensusObjectV2::ClassRegistered {
            class_id,
            artifact_root,
            slash_value_per_pwu,
            pwu_rule,
            initial_target,
            share_permille,
            activation_daa,
            admission: Some(admission),
        } = &mut object
        else {
            unreachable!()
        };
        let message = kaspa_consensus_core::palw_state_v2::palw_class_registration_message_v2(
            chain.network_domain,
            *class_id,
            *share_permille,
            *activation_daa,
            &admission.registrant_bond,
            *artifact_root,
            *slash_value_per_pwu,
            *initial_target,
            pwu_rule,
            &admission.canonical,
        );
        admission.signature =
            signed(0, message.as_byte_slice(), kaspa_consensus_core::palw_state_v2::PALW_CLASS_REGISTRATION_V2_MLDSA87_CONTEXT);
        chain.send(0, &object, &mut floats[0], &[]).await;
        assert_eq!(
            chain.tc.palw_registered_class_carriage_v1(self.profile.shape_profile_id()),
            Some((self.profile.clone(), self.canonical.clone()))
        );
        eprintln!(
            "[service-tiled] ordinary signed class registration accepted; root {}, {} lanes, canonical {}/{}",
            self.root, self.profile.vocab_size, self.canonical.declared_prefill_tokens, self.canonical.exact_decode_tokens
        );
    }

    /// The canonical graph is first published through the ordinary certification object.
    /// A qualified cache relation cannot be attached to an unproven profile at registration.
    pub async fn declare_semantics(&self, chain: &mut Chain, floats: &mut [Funding]) {
        if chain.config.params.palw_class_semantics_v1_active_at(chain.daa()) {
            use kaspa_consensus_core::palw_class_semantics_v1::*;
            let declaration = PalwClassSemanticsDeclarationV1 {
                version: PALW_CLASS_SEMANTICS_VERSION_V1,
                class_id: self.profile.shape_profile_id(),
                artifact_root: self.root,
                model_profile_id: self.canonical.model_profile_id,
                runtime_manifest_hash: self.canonical.runtime_manifest_hash,
                runtime_class_id: self.canonical.runtime_class_id,
                tokenizer_id: self.canonical.tokenizer_id,
                cu_ruleset_id: self.canonical.cu_ruleset_id,
                token_table_root: self.table.root(),
                rendering_rule: kaspa_consensus_core::palw_tokenizer_rendering_v1::PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1,
                raw_tokenizer_bytes: std::fs::metadata(self.path.parent().unwrap().join("tokenizer.json")).unwrap().len(),
                tokenizer_key_events: kaspa_consensus_core::palw_tokenizer_rendering_v1::palw_tokenizer_key_events_v1(
                    &std::fs::read(self.path.parent().unwrap().join("tokenizer.json")).unwrap(),
                )
                .unwrap(),
                prefix_state_relation: Some(PalwPrefixStateRelationV1::DenseInt32KvV1),
            };
            let message = palw_class_semantics_message_v1(chain.network_domain, &declaration);
            let object = PalwConsensusObjectV2::ClassSemanticsDeclaredV1 {
                declaration: Box::new(declaration),
                signature: signed(0, message.as_byte_slice(), PALW_CLASS_SEMANTICS_SIGNATURE_CONTEXT_V1),
            };
            chain.send(0, &object, &mut floats[0], &[]).await;
        }
    }

    pub fn produce(
        &self,
        chain: &Chain,
        fault: u8,
    ) -> (
        kaspa_consensus_core::palw_backend::PalwExecutionOutcomeV1,
        kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1,
        Option<(kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptCommitmentV3, Vec<u32>)>,
        std::collections::BTreeMap<
            kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1,
            kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1,
        >,
    ) {
        self.produce_for_card(chain, 0, Some(fault))
    }

    pub(super) fn produce_for_card(
        &self,
        chain: &Chain,
        card: usize,
        fault: Option<u8>,
    ) -> (
        kaspa_consensus_core::palw_backend::PalwExecutionOutcomeV1,
        kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1,
        Option<(kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptCommitmentV3, Vec<u32>)>,
        std::collections::BTreeMap<
            kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1,
            kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1,
        >,
    ) {
        use kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1;
        use kaspa_consensus_core::palw_fp_constraint_job_v1::*;
        use kaspa_consensus_core::palw_freeprompt_v3::*;
        use kaspa_consensus_core::palw_step_refute::*;
        let backend = self.backend(chain);
        let facts = chain.tc.palw_producer_facts_v2(chain.bundle.base_class_id, Some(chain.bonds[card].0)).unwrap();
        let ids = vec![0, 1, 2];
        let mut job = PalwFreePromptJobV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: chain.network_domain,
            class_id: self.profile.shape_profile_id(),
            executor_bond: chain.bonds[card].0,
            executor_pubkey: card_pubkey(card),
            operator_id: facts.bond.as_ref().unwrap().operator_id,
            anchor_block: facts.chain_point,
            anchor_daa: facts.daa_score,
            job_nonce: [0xA6; 32],
            tokenizer_id: self.table.tokenizer_id,
            prompt_token_ids_hash: kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(
                kaspa_consensus_core::palw_prompt_ids_v1::palw_prompt_ids_form_of_class_v1(
                    chain.config.params.palw_prompt_ids_form_v1(),
                    &self.profile,
                ),
                &ids,
            )
            .unwrap(),
            prompt_tokens: ids.len() as u32,
            decode_token_limit: 4,
            max_context_tokens: self.profile.n_ctx,
            privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: PALW_FP_PROMPT_MODE_USER,
            sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
            temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
            decode: None,
            tail: None,
        };
        let prefix_fault = matches!(fault, Some(5) | Some(6));
        if prefix_fault {
            let true_prefix = backend.prefix_state_v1(self.profile.shape_profile_id(), self.table.tokenizer_id, &ids[..2]).unwrap();
            assert_eq!(true_prefix.prefix_tokens, 2);
            job.version = if fault == Some(6) {
                kaspa_consensus_core::palw_fp_prefix_v1::PALW_FP_PREFIX_INHERIT_VERSION
            } else {
                kaspa_consensus_core::palw_fp_prefix_v1::PALW_FP_PREFIX_VERSION
            };
            job.decode = Some(kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4::NOOP);
            job.tail =
                Some(PalwFpJobTailV1::Prefix(PalwFpPrefixStateV1 { state_root: Hash64::from_u64_word(0xBAD17), ..true_prefix }));
            assert!(backend.verify_prefix_state_v1(&job, &ids).is_err(), "the actual holder rejects the false named prefix");
        } else {
            // Use the same DFA as the flat fixture, with the actual tokenizer-derived table.
            selection_constraint_job_with_table(&mut job, self.table.clone());
        }
        if fault == Some(4) {
            super::terminal::enlarge_selection_job_v1(&mut job);
        }
        let mask = palw_fp_constraint_mask_for_host_v1(&job, Some(self.table.clone())).unwrap();
        let run = palw_fp_with_constraint_scope_v1(mask, || {
            backend.execute_free_prompt(&job, &ids.iter().map(|id| *id as usize).collect::<Vec<_>>())
        })
        .unwrap();
        let retained = misaka_palw_base0::produce::base0_material_decode_any_v1(&run.outcome.material).unwrap();
        let mut binding = retained.binding().clone();
        assert_eq!(binding.job_context.tokenizer_id, self.table.tokenizer_id);
        let rows = retained.logits_rows();
        let mut generated = retained.generated_token_ids().to_vec();
        assert_eq!(generated.len(), 4);
        if !prefix_fault {
            assert!(generated.iter().all(|id| *id == 0 || *id == PALW_LOGITS_TILE_LANES as u32));
        }
        generated[2] = match fault {
            Some(1) => 2,
            Some(2) => self.profile.vocab_size - 1,
            Some(3) | Some(4) => {
                if generated[2] == 0 {
                    PALW_LOGITS_TILE_LANES as u32
                } else {
                    0
                }
            }
            None | Some(5) | Some(6) => generated[2],
            _ => unreachable!(),
        };
        binding.full_logits_trace_root = tiled_logits_trace_root_v1(&binding.job_context, rows, &generated).unwrap();
        binding.committed_execution_root = kaspa_consensus_core::palw_step_leg::binding_commitment_root_v1(&binding);
        kaspa_consensus_core::palw_step_leg::verify_binding_v1(&binding).unwrap();
        let mut commitment = kaspa_consensus_core::palw_fp_execution_v3::palw_fp_commitment_from_context_v3(
            &job,
            &binding.job_context,
            &run,
            chain.daa() + facts.min_trace_retention_daa,
        )
        .unwrap();
        commitment.execution_root = binding.committed_execution_root;
        commitment.trace_root = binding.full_logits_trace_root;
        commitment.output_root =
            kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_output_root_v1(&binding.job_context, &generated);
        if fault.is_none() {
            assert_eq!(
                (commitment.execution_root, commitment.trace_root, commitment.output_root),
                (run.outcome.execution_root, run.outcome.trace_root, run.outcome.output_root),
                "metadata-only faults retain the actual producer computation and all three roots"
            );
        }
        let initial = logits_event_disclosure_v1(&binding, rows, &generated, 0, 0).unwrap();
        let mut events = std::collections::BTreeMap::new();
        // The native DA session also draws a unit beside the named binding. Answer this
        // authenticated initial-row tile before testing withholding at the selected row.
        events.insert(PalwDaUnitV1::Event { row: 0, tile: 1 }, logits_event_disclosure_v1(&binding, rows, &generated, 0, 1).unwrap());
        for tile in 0..=u8::from(matches!(fault, Some(3) | Some(4))) {
            events.insert(
                PalwDaUnitV1::Event { row: 2, tile },
                logits_event_disclosure_v1(&binding, rows, &generated, 2, tile).unwrap(),
            );
        }
        let mut outcome = run.outcome;
        outcome.execution_root = commitment.execution_root;
        outcome.trace_root = commitment.trace_root;
        outcome.output_root = commitment.output_root;
        outcome.material.clear();
        // Only signed public answer candidates leave the producer side. Neither its backend
        // nor its capture/row array enters the verifier's service or proof builder.
        (outcome, initial, Some((commitment, ids)), events)
    }

    /// Build only the signed-public CKW payloads from producer retention. The caller stores
    /// these beside its public answer candidates; no capture/backend enters the outsider service.
    pub fn prefix_kernel_answers(
        &self,
        chain: &Chain,
        commitment: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
        ids: &[u32],
    ) -> std::collections::BTreeMap<
        kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyHeldUnitV2,
        (kaspa_consensus_core::palw_step_leg::PalwStepBindingV2, kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyHeldAnswerV2),
    > {
        use kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyHeldUnitV2;
        let backend = self.backend(chain);
        let job = &commitment.job;
        assert!(job.is_prefix_state());
        let run = backend.execute_free_prompt(job, &ids.iter().map(|id| *id as usize).collect::<Vec<_>>()).unwrap();
        assert_eq!(run.outcome.execution_root, commitment.execution_root);
        assert_eq!(run.outcome.trace_root, commitment.trace_root);
        assert_eq!(run.outcome.output_root, commitment.output_root);
        let retained = misaka_palw_base0::produce::base0_material_decode_any_v1(&run.outcome.material).unwrap();
        let binding = retained.binding();
        let named = kaspa_consensus_core::palw_fp_prefix_v1::palw_fp_prefix_shape_v1(job).unwrap();
        let plan =
            kaspa_consensus_core::palw_prefix_state_court_v1::PalwPrefixKvPlanV1::of_profile(&self.profile, named.prefix_tokens)
                .unwrap();
        let roots = kaspa_consensus_core::palw_backend::PalwClaimRootsV1 {
            execution_root: commitment.execution_root,
            trace_root: commitment.trace_root,
            output_root: Some(commitment.output_root),
            anchor: kaspa_consensus_core::palw_freeprompt_v3::fp_canonical_anchor_v1(job),
            attempt_draw: None,
            job_pin: Some(kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_v1(commitment)),
        };
        let form = kaspa_consensus_core::palw_prompt_ids_v1::palw_prompt_ids_form_of_class_v1(
            chain.config.params.palw_prompt_ids_form_v1(),
            &self.profile,
        );
        let mut answers = std::collections::BTreeMap::new();
        for ordinal in 0..plan.tile_count {
            let (coordinate, _) = plan.coordinate(&self.profile, ordinal).unwrap();
            let leaf =
                kaspa_consensus_core::palw_step::canonical_step_leaf_index(&self.profile, &binding.job_context, &coordinate).unwrap();
            let unit = PalwLegacyHeldUnitV2::KernelWitness { leaf };
            let payload = crate::palw_legacy_held_v2::palw_legacy_held_capture_answer_v2(
                backend.as_ref(),
                &run.outcome.material,
                ids,
                roots,
                form,
                &unit,
            )
            .unwrap();
            answers.insert(unit, payload);
        }
        assert_eq!(answers.len() as u64, plan.tile_count);
        answers
    }

    pub fn readiness(&self, chain: &Chain, card: usize) -> PalwConsensusObjectV2 {
        use kaspa_consensus_core::palw_model_registry_v1::*;
        let read = chain.tc.palw_model_registry_v1().expect("the ordinary T12 registry is active");
        assert!(read.active && chain.config.params.palw_readiness_v2_at(chain.daa()));
        let class_id = self.profile.shape_profile_id();
        let span = chain.daa() / read.span_daa;
        let bond = chain.bonds[card];
        let bond_bytes = borsh::to_vec(&bond).unwrap();
        // Each seat loads the registered model independently and opens the native challenge.
        // This is possession evidence, not a fabricated readiness row or production exemption.
        let backend = self.backend(chain);
        let (root, leaves) = backend.artifact_root_and_leaf_count().unwrap();
        assert_eq!(root, self.root);
        let draw = palw_readiness_v2_draw_v1(&palw_readiness_v2_challenge_seed_v1(&class_id, &bond_bytes, span), leaves);
        let mut stream = kaspa_consensus_core::palw_artifact::PalwArtifactMultiproofStreamV1::new(leaves, &draw).unwrap();
        let (held_root, held_leaves, drawn) = backend
            .artifact_readiness_material_streamed_v1(&draw, &mut |leaf| stream.push(leaf))
            .expect("A16 streams its possession inventory")
            .unwrap();
        assert_eq!((held_root, held_leaves), (root, leaves));
        let mut opened = Vec::new();
        let mut bytes = 0;
        for (index, operand) in drawn {
            if bytes >= PALW_READINESS_V2_BUDGET_BYTES_V1 {
                break;
            }
            assert!(operand.bytes.len() <= PALW_READINESS_V2_LEAF_MAX_BYTES_V1);
            bytes += operand.bytes.len();
            opened.push((index, operand));
        }
        palw_readiness_v2_opening_is_the_challenge_v1(
            &draw,
            &opened.iter().map(|(index, operand)| (*index, operand.bytes.len())).collect::<Vec<_>>(),
        )
        .unwrap();
        opened.sort_by_key(|(index, _)| *index);
        let proof = stream.finish(&opened).unwrap();
        kaspa_consensus_core::palw_artifact::verify_artifact_multiproof_v1(&proof, self.root).unwrap();
        let message = palw_seat_readiness_message_v2(chain.network_domain, &bond_bytes, &class_id, span, &proof);
        PalwConsensusObjectV2::SeatReadinessProvedV2 {
            bond,
            class_id,
            span,
            proof: Box::new(proof),
            signature: signed(card, message.as_byte_slice(), PALW_SEAT_READINESS_V2_MLDSA87_CONTEXT),
        }
    }

    pub async fn refresh_panel_readiness(&self, chain: &mut Chain, floats: &mut [Funding]) {
        // Jury possession expires before accepted_daa + anchor_delay on this native span
        // clock. The colluding model holders refresh through ordinary signed carriers.
        #[allow(clippy::needless_range_loop)] // `card` is the bond index and the seat number
        for card in 1..chain.bonds.len() {
            let object = self.readiness(chain, card);
            chain.send(card, &object, &mut floats[card], &[]).await;
        }
    }

    pub async fn prepare_panel(&self, chain: &mut Chain, floats: &mut [Funding]) {
        let class_id = self.profile.shape_profile_id();
        #[allow(clippy::needless_range_loop)] // `card` is the bond index and the seat number
        for card in 1..chain.bonds.len() {
            let capable_classes = BTreeSet::from([chain.bundle.base_class_id, class_id]);
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
        // Leave enough time for all seven independent possession carriers, then supply a
        // normal winning floor anchor in the span immediately before the class's own jury.
        let audit = (span + 12..=span + 12 + period)
            .find(|s| kaspa_consensus_core::palw_activation_pool_v1::palw_admission_audit_due_staggered_v1(&class_id, *s, period))
            .unwrap();
        chain.advance((audit - 10) * read.span_daa).await;
        #[allow(clippy::needless_range_loop)] // `card` is the bond index and the seat number
        for card in 1..chain.bonds.len() {
            let object = self.readiness(chain, card);
            chain.send(card, &object, &mut floats[card], &[]).await;
        }
        chain.advance((audit - 1) * read.span_daa).await;
        let (anchor, _) = chain.build_attempt_drawn(0, chain.config.params.target_time_per_block(), vec![], &|_| true, true);
        chain.insert(anchor).await;
        chain.advance((audit + 2) * read.span_daa).await;
        let read = chain.tc.palw_model_registry_v1().unwrap();
        let class = read.classes.iter().find(|c| c.class_id == class_id).unwrap();
        eprintln!(
            "[service-tiled] normal jury at span {audit}: state {:?}, ready {}, room {}, reason {}",
            class.row.as_ref().map(|r| r.state),
            class.ready_seats_now,
            class.panel_room,
            class.reason
        );
        assert!(
            !matches!(
                class.row.as_ref().unwrap().state,
                kaspa_consensus_core::palw_model_registry_v1::PalwModelLifecycleV1::Candidate
            ),
            "a bought class must pass its native outside jury"
        );
        assert!(class.row.as_ref().unwrap().state.admission_permille() > 0);
        assert!(class.panel_room > 0 && class.ready_seats_now >= 5);
    }
}

pub(super) async fn carry_public_rpc_answer(
    chain: &mut Chain,
    manager: &Arc<ConsensusManager>,
    pool: &MiningManagerProxy,
    flow: &Arc<FlowContext>,
    object: &PalwConsensusObjectV2,
    funding: &mut Funding,
    payer: usize,
) {
    // `payer`: whose key funds the carrier (the object itself is signed separately). Under the automatic recovery runs the producer
    // service spends bond 0's float itself, so the harness pays from a spare card's float instead of re-spending it (lane FPCP).
    let tx = palw_carrier_fees::build_lifecycle_carrier_v2(
        &chain.config,
        key(payer),
        object,
        funding.0,
        &funding.1,
        &[],
        None,
        chain.daa(),
    )
    .unwrap();
    let session = manager.consensus().unguarded_session();
    flow.submit_rpc_transaction(&session, tx.clone(), kaspa_mining::mempool::tx::Orphan::Forbidden)
        .await
        .expect("the signed selected tile passes ordinary H1/PQ admission");
    let mut included = false;
    for _ in 0..8 {
        let block = pool_heartbeat(chain, manager, pool).await;
        if block.transactions.iter().any(|t| t.id() == tx.id()) {
            included = true;
            break;
        }
    }
    assert!(included, "the selected tile is actually included in the production template");
    pool_heartbeat(chain, manager, pool).await;
    let index = tx.outputs.len() - 1;
    let out = &tx.outputs[index];
    *funding = (
        TransactionOutpoint::new(tx.id(), index as u32),
        UtxoEntry::new(out.value, out.script_public_key.clone(), chain.daa(), false),
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn registered_tiled_constraint_class_passes_normal_registration_and_host_resolution() {
    let (config, bundle, premine, mut floats) = fixture_config_with(false, true, true, true, true);
    let mut chain = Chain::new(config, bundle, &premine);
    chain.advance(2).await;
    let fixture = TiledFixture::new(&chain);
    fixture.register(&mut chain, &mut floats).await;
    certify_fp_lane(&mut chain, &mut floats, fixture.profile.clone(), true).await;
    fixture.declare_semantics(&mut chain, &mut floats).await;
    let backend = fixture.backend(&chain);
    let (context, _) = backend.job_for_anchor(Hash64::from_u64_word(0xC014)).unwrap();
    assert_eq!(context.shape_profile_id, fixture.profile.shape_profile_id());
    let (_, event, _, selected) = fixture.produce(&chain, 3);
    assert_eq!(event.binding().job_context.tokenizer_id, fixture.table.tokenizer_id);
    assert_eq!(selected.keys().filter(|u| matches!(u, PalwDaUnitV1::Event { row: 2, .. })).count(), 2);
    assert!(selected.contains_key(&PalwDaUnitV1::Event { row: 0, tile: 1 }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn registered_tiled_constraint_class_passes_normal_readiness_jury_and_fp_price() {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, mut floats) = fixture_config_with(false, true, true, true, true);
    let mut chain = Chain::new(config, bundle, &premine);
    chain.advance(2).await;
    let fixture = TiledFixture::new(&chain);
    fixture.register(&mut chain, &mut floats).await;
    certify_fp_lane(&mut chain, &mut floats, fixture.profile.clone(), true).await;
    fixture.declare_semantics(&mut chain, &mut floats).await;
    fixture.prepare_panel(&mut chain, &mut floats).await;
    let (_, _, Some((commitment, ids)), _) = fixture.produce(&chain, 3) else { unreachable!() };
    let quote = chain
        .tc
        .palw_fp_commitment_price_v1(
            commitment.job.class_id,
            ids.clone(),
            commitment.job.prompt_tokens,
            commitment.decode_tokens_executed,
            commitment.work_leaves,
            Some(chain.bonds[0].0),
        )
        .unwrap()
        .price
        .expect("the normally jury-admitted bought class accepts the ordinary FP price query");
    assert!(quote.quanta > 0 && quote.pwu > 0);
    eprintln!("[service-tiled] native bought-class FP price accepted: {} quanta, {} pwu", quote.quanta, quote.pwu);
    let claim = carry_fp_commitment(&mut chain, &mut floats, &commitment, &ids, false).await;
    fixture.refresh_panel_readiness(&mut chain, &mut floats).await;
    let slot = chain.view(claim).accepted_daa + chain.bundle.panel.anchor_delay();
    chain.advance(slot).await;
    let (anchor, _) = chain.build_attempt_drawn(7, chain.config.params.target_time_per_block(), vec![], &|_| true, true);
    chain.insert(anchor).await;
    let view = chain.view(claim);
    eprintln!("[service-tiled] native panel stage at DAA {}: {:?}", chain.daa(), view);
    let duties = chain.tc.palw_seat_duties_v2(chain.bonds.clone()).into_iter().filter(|d| d.claim_id == claim).collect::<Vec<_>>();
    assert_eq!(duties.len(), 5, "the ordinary bought-class FP must bind all five colluding panel seats");
}
