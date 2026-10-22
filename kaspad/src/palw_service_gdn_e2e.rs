//! GDN node fixtures use a real independently loaded hybrid artifact and ordinary signed registration.
use super::*;
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;

struct GdnFixture {
    _dir: tempfile::TempDir,
    path: std::path::PathBuf,
    profile: PalwShapeProfileV3,
    canonical: PalwJobContextV2,
    root: Hash64,
}

impl GdnFixture {
    fn new(chain: &Chain) -> Self {
        use kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v8;
        use misaka_palw_base0::qwen36::{Qwen36Writer, qwen36_dev_fixture};
        use misaka_palw_base0::qwen36_backend::Qwen36Backend;
        let artifact = Arc::new(qwen36_dev_fixture(4, 8));
        let mut geometry = crate::palw_gdn_held_filer::tests::geometry();
        // The artifact's rotary table is finite, and the attention court opens
        // one head per tile. The fold-only GDN fixture's broad geometry is not
        // an ordinary node-registration declaration.
        geometry.n_ctx = artifact.shape.max_position as u32;
        geometry.tile_len = geometry.attn_head_dim;
        let profile = qwen36_profile_v8(geometry).unwrap();
        let root = misaka_palw_base0::inventory::qwen36_inventory_v1(&artifact, &profile).unwrap().root();
        let backend = Qwen36Backend::from_registered_profile(
            artifact.clone(),
            chain.config.params.net.to_string().into_bytes(),
            profile.clone(),
            kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_canonical_v1(&profile, false).unwrap(),
        )
        .unwrap()
        .with_prompt_ids_form(chain.config.params.palw_prompt_ids_form_v1());
        let (canonical, _) = backend.job_for_anchor(Hash64::default()).unwrap();
        assert_eq!(canonical.max_context_tokens, profile.n_ctx, "the registered job uses the artifact's actual context bound");
        assert!(kaspa_consensus_core::palw_class_admission_v2::palw_fused_sites_are_dissectable_v1(&profile));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("outside-hybrid.palwq36");
        let plan: Vec<(String, usize)> =
            artifact.tensor_names().iter().map(|name| (name.to_string(), artifact.tensor(name).unwrap().len())).collect();
        let mut writer = Qwen36Writer::create(&path, &artifact.shape, &artifact.rope, artifact.params_map(), plan.clone()).unwrap();
        for (name, _) in &plan {
            writer.push(name, &artifact.tensor(name).unwrap()).unwrap();
        }
        writer.finish().unwrap();
        Self { _dir: dir, path, profile, canonical, root }
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
        let shape = kaspa_consensus_core::palw_class_admission_v2::palw_admission_shape_at_v1(
            &chain.config.params,
            &chain.bundle,
            &self.profile,
            chain.daa(),
        )
        .unwrap();
        kaspa_consensus_core::palw_class_admission_v2::palw_admission_probe_v1(
            &chain.bundle,
            &self.profile,
            &self.canonical,
            self.root,
            &[],
            &shape,
        )
        .unwrap_or_else(|why| panic!("the ordinary GDN registration preflight refuses: {why:?}"));
        kaspa_consensus_core::palw_attempt_rules_v1::palw_registration_attribution_v2(
            &self.profile,
            &self.canonical,
            chain.config.params.palw_offence_attribution_active_at(chain.daa()),
            shape.prompt_ids_form,
            shape.memory_routing_court,
        )
        .unwrap_or_else(|why| panic!("the ordinary GDN attribution gate refuses: {why:?}"));
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
        let registration = chain.send(0, &object, &mut floats[0], &[]).await;
        assert!(chain.history.iter().any(|block| block.transactions.iter().any(|tx| tx.id() == registration.id())),
            "the ordinary template must actually carry the GDN registration");
        assert!(chain.tc.palw_v2_class_table().iter().any(|row| row.class_id == self.profile.shape_profile_id()),
            "the carried GDN registration must enter the class table at DAA {}", chain.daa());
        assert_eq!(
            chain.tc.palw_registered_class_carriage_v1(self.profile.shape_profile_id()),
            Some((self.profile.clone(), self.canonical.clone()))
        );
        eprintln!(
            "[service-gdn] ordinary signed class registration accepted; root {}, {} lanes, canonical {}/{}",
            self.root, self.profile.vocab_size, self.canonical.declared_prefill_tokens, self.canonical.exact_decode_tokens
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn registered_gdn_class_passes_normal_registration_and_host_resolution() {
    let (config, _bundle, premine, mut floats) = fixture_config_with(false, true, true, false, false);
    // This graph reaches the token-lift kernel; the ordinary admission gate requires its fence.
    let mut params = config.params.clone();
    params.palw_token_lift = Some(ForkActivation::new(0));
    params.palw_gdn_key_heads = Some(ForkActivation::new(0));
    // The registration graph is retained in native routing aux rows. This
    // fixture starts that route at genesis, so its policy must be in force too.
    params.palw_probabilistic_constraints_v1 = Some(ForkActivation::new(0));
    params.palw_memory_routing_court_v2 = Some(ForkActivation::new(0));
    params.sync_palw_memory_routing_court_v2();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { unreachable!() };
    let bundle = bundle.clone();
    assert_eq!(bundle.state.memory_routing_court_from_daa(), Some(0));
    let mut chain = Chain::new(Config::new(params), bundle, &premine);
    chain.advance(2).await;
    let fixture = GdnFixture::new(&chain);
    fixture.register(&mut chain, &mut floats).await;
    let backend = fixture.backend(&chain);
    assert!(backend.as_any_v1().and_then(|a| a.downcast_ref::<misaka_palw_base0::qwen36_backend::Qwen36Backend>()).is_some());
    let (context, _) = backend.job_for_anchor(Hash64::from_u64_word(0x6D14)).unwrap();
    assert_eq!(context.shape_profile_id, fixture.profile.shape_profile_id());
}
