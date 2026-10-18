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
        use kaspa_consensus_core::palw_qwen36_profile::{qwen36_held_canonical_v1, qwen36_profile_v8};
        use misaka_palw_base0::qwen36::{Qwen36Writer, qwen36_dev_fixture};
        use misaka_palw_base0::qwen36_backend::Qwen36Backend;
        let artifact = Arc::new(qwen36_dev_fixture(4, 8));
        let profile = qwen36_profile_v8(crate::palw_gdn_held_filer::tests::geometry()).unwrap();
        let root = misaka_palw_base0::inventory::qwen36_inventory_v1(&artifact, &profile).unwrap().root();
        let backend = Qwen36Backend::from_registered_profile(
            artifact.clone(),
            chain.config.params.net.to_string().into_bytes(),
            profile.clone(),
            qwen36_held_canonical_v1(160),
        )
        .unwrap()
        .with_prompt_ids_form(chain.config.params.palw_prompt_ids_form_v1());
        let (canonical, _) = backend.job_for_anchor(Hash64::default()).unwrap();
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
            "[service-gdn] ordinary signed class registration accepted; root {}, {} lanes, canonical {}/{}",
            self.root, self.profile.vocab_size, self.canonical.declared_prefill_tokens, self.canonical.exact_decode_tokens
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn registered_gdn_class_passes_normal_registration_and_host_resolution() {
    let (config, bundle, premine, mut floats) = fixture_config_with(false, true, true, false, false);
    // This graph reaches the token-lift kernel; the ordinary admission gate requires its fence.
    let mut params = config.params.clone();
    params.palw_token_lift = Some(ForkActivation::new(0));
    params.palw_gdn_key_heads = Some(ForkActivation::new(0));
    let mut chain = Chain::new(Config::new(params), bundle, &premine);
    chain.advance(2).await;
    let fixture = GdnFixture::new(&chain);
    fixture.register(&mut chain, &mut floats).await;
    let backend = fixture.backend(&chain);
    assert!(backend.as_any_v1().and_then(|a| a.downcast_ref::<misaka_palw_base0::qwen36_backend::Qwen36Backend>()).is_some());
    let (context, _) = backend.job_for_anchor(Hash64::from_u64_word(0x6D14)).unwrap();
    assert_eq!(context.shape_profile_id, fixture.profile.shape_profile_id());
}
