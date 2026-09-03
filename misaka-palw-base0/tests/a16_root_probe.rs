//! **Which root form does the on-disk A16 artifact actually derive?** (Relaunch 5 triage)
//!
//! The chain registers `PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT` for the graph-v2 A16 row, but
//! `CanonicalClassV1::artifact_root` derives the court-capable row's root as the A16
//! operand-inventory root, not the flat `artifact_digest()`. Prints both for a file so the two
//! spellings can be compared against the pin. Ignored: it needs a 1.7 GiB file named by
//! `PALW_A16_PATH`.
#[test]
#[ignore]
fn print_a16_root_forms() {
    let path = std::env::var("PALW_A16_PATH").expect("PALW_A16_PATH=/path/to/qwen25-1.5b-a16.palwart");
    let bytes = std::fs::read(&path).expect("read the artifact");
    let artifact = misaka_palw_base0::artifact::decode_artifact_file_v1(&bytes).expect("decode .palwart");
    let profile =
        kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v2(kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B_A16)
            .expect("graph-v2 A16 profile projects");
    let digest = artifact.artifact_digest();
    let inventory = misaka_palw_base0::inventory::a16_inventory_v1(&artifact, &profile).expect("inventory builds").root();
    println!("file            {path}");
    println!("class_id(v2)    {}", profile.shape_profile_id());
    println!("artifact_digest {digest}");
    println!("inventory_root  {inventory}");
    println!("pinned_const    {}", kaspa_consensus_core::config::params::PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT);
    println!("digest==pin     {}", digest == kaspa_consensus_core::config::params::PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT);
    println!("inventory==pin  {}", inventory == kaspa_consensus_core::config::params::PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT);
}

/// **The shipped A16 row is the court-capable graph-v2 row, so its registered root MUST be the
/// inventory form** — the structural half of the probe above, runnable without the 1.7 GiB file.
///
/// `CanonicalClassV1::artifact_root` picks the root form by `state_chunk_map_id`: the four-byte
/// integer-KV map means "court-capable, register the operand-inventory root", anything else means
/// "v1 row, register the flat digest". The genesis card pinned the digest for a profile whose map
/// is the four-byte one, which is precisely the mismatch this asserts can never be silent again:
/// if this test holds, `PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT` is an inventory root by contract,
/// and the ignored probe is how a human checks the value against the file.
#[test]
fn the_shipped_a16_row_registers_the_inventory_root_form() {
    let profile =
        kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v2(kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B_A16)
            .expect("graph-v2 A16 profile projects");
    assert_eq!(
        profile.state_chunk_map_id,
        kaspa_consensus_core::palw_state_chunk_map::integer_kv_state_chunk_map_id_v2(),
        "the registered A16 row is court-capable, so its root form is the operand-inventory root — \
         `PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT` must be `a16_inventory_v1(..).root()`, never `artifact_digest()`"
    );
    // The digest of the deployed file is a known constant; the pin must NOT be it.
    let digest_form = "c00faa480f2344d4a737e5b2e87ab6064d8d6e42c1ffeb6aa0a14ed62134299a7c9dc08f15342cefca1e29390810e6d2c5879f4c3853ebe43a9e2d47ed57ba17";
    assert_ne!(
        kaspa_consensus_core::config::params::PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT.to_string(),
        digest_form,
        "the pin is the flat digest again — that is the two-mappings defect, re-pin to the inventory root"
    );
}

// =================================================================================================
// ADR-0082 fix G — the graph-v5 512 row the testnet-11 5f genesis registers
// =================================================================================================

/// **The four values the v5 genesis pin is chosen from, MEASURED over both real files.**
///
/// The v2 pin (`1a7457f1…`) was measured by [`print_a16_root_forms`] over the deployed file under
/// the graph-**v2** profile. The v5 row registers the same weights under a different graph, and a
/// different file is on the table (the tokenizer-bound conversion the cut ships), so four values
/// have to be seen at once rather than reasoned about:
///
/// * `artifact_digest()` of the shipped file and of the bound file — these MUST differ, because
///   binding writes the tokenizer commitment field;
/// * `a16_inventory_v1(file, v5_profile).root()` of each — these are what a registration pins, and
///   the claim under test is that they are EQUAL to each other (the commitment field is not an
///   operand, so the inventory does not cover it) and equal to the v2 pin (the inventory enumerates
///   operands, and Decision 1 fuses nodes without moving a tensor).
///
/// Ignored: it reads two 1.7 GiB files. Run with
/// `PALW_A16_PATH=/Users/wata/Downloads/qwen25-1.5b-a16.palwart PALW_A16_BOUND_PATH=…/instruct-bound.palwart`.
#[test]
#[ignore]
fn print_a16_v5_root_forms() {
    let shipped = std::env::var("PALW_A16_PATH").expect("PALW_A16_PATH=/path/to/qwen25-1.5b-a16.palwart");
    let bound = std::env::var("PALW_A16_BOUND_PATH").expect("PALW_A16_BOUND_PATH=/path/to/instruct-bound.palwart");
    let v5 = kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v5(512).expect("the v5 row projects");
    let v2 =
        kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v2(kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B_A16)
            .expect("the registered graph-v2 row projects");
    println!("v5 profile class id {}", v5.shape_profile_id());
    println!("v2 profile class id {}", v2.shape_profile_id());
    let mut inventories = Vec::new();
    for (name, path) in [("shipped", &shipped), ("bound", &bound)] {
        let bytes = std::fs::read(path).expect("read the artifact");
        let artifact = misaka_palw_base0::artifact::decode_artifact_file_v1(&bytes).expect("decode .palwart");
        let digest = artifact.artifact_digest();
        let inv_v5 = misaka_palw_base0::inventory::a16_inventory_v1(&artifact, &v5).expect("inventory builds").root();
        let inv_v2 = misaka_palw_base0::inventory::a16_inventory_v1(&artifact, &v2).expect("inventory builds").root();
        println!("[{name}] {path}");
        println!("[{name}] artifact_digest       {digest}");
        println!("[{name}] inventory_root(v5)    {inv_v5}");
        println!("[{name}] inventory_root(v2)    {inv_v2}");
        println!("[{name}] v5==v2 inventory      {}", inv_v5 == inv_v2);
        println!(
            "[{name}] v5==the v2 genesis pin {}",
            inv_v5 == kaspa_consensus_core::config::params::PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT
        );
        inventories.push((name, inv_v5));
    }
    println!("shipped inventory(v5) == bound inventory(v5): {}", inventories[0].1 == inventories[1].1);
    // (the pin itself is asserted by `the_v5_genesis_pin_is_the_measured_inventory_root` below)
}

/// **Everything the v5 genesis row is derived from, printed under the ruleset that registers it.**
///
/// No file needed: these are properties of the graph and of the shipped RC bundle. It exists
/// because every number the registration publishes has to be seen under the configuration it was
/// measured in — the ladder, the court's arity, the close in chunks — and a report that quotes one
/// under another is the defect ADR-0082 §1 opens with.
#[test]
fn print_the_graph_v5_genesis_row_numbers() {
    use kaspa_consensus_core::palw_class_admission_v2 as adm;
    use kaspa_consensus_core::palw_context_ladder as ladder;

    let row = misaka_palw_base0::classes::a16_graph_v5_row_v1().expect("this build tables the graph-v5 row");
    let profile = row.profile.clone();
    let rc_ladder = kaspa_consensus_core::palw_fp_devnet_v3::COURT_MAX_STEP_LEAVES;
    let canonical = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, row.canonical_job.0, row.canonical_job.1);
    println!("class id            {}", row.class_id());
    println!("model id            {}", row.model_id);
    println!("n_ctx               {}", profile.n_ctx);
    println!("canonical job       {:?}", row.canonical_job);
    println!("ruleset ladder      {rc_ladder}");
    let counted = kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&profile, &canonical, rc_ladder);
    let worst = kaspa_consensus_core::palw_step::worst_case_step_leaf_count_capped_v1(&profile, rc_ladder);
    println!("canonical leaves    {counted:?}");
    println!("worst case leaves   {worst:?}");
    let genesis_anchored =
        adm::derive_court_cost_shaped_v1(&profile, adm::PalwCourtCostShapeV1::genesis_anchored_v1(&profile, rc_ladder));
    println!("cost, genesis-anchored (the shipped court): {genesis_anchored:?}");
    if let Ok(cost) = &genesis_anchored {
        println!("  close chunks {}", kaspa_consensus_core::palw_mode_v2::palw_close_chunks_for_bytes_v1(cost.max_close_bytes));
    }

    let bundle = match kaspa_consensus_core::config::params::palw_rc_shipped_params().palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) => b,
        other => panic!("the RC preset is not a v2 bundle: {other:?}"),
    };
    println!(
        "RC court: ladder {} arity {} close_chunks {} close_bytes {} terminal_macs {} operands {} turn_deadline {} window_court {}",
        bundle.court.max_step_leaf_count(),
        bundle.court.dissection_arity(),
        bundle.court.max_close_chunks(),
        bundle.court.max_close_bytes(),
        bundle.court.max_terminal_macs(),
        bundle.court.max_operand_count(),
        bundle.court.turn_deadline_daa(),
        bundle.state.window_court()
    );
    let kary = adm::PalwKaryCourtV1 {
        dissection_arity: bundle.court.dissection_arity(),
        prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
        window_court_daa: bundle.state.window_court(),
    };
    if let Some(rules) = ladder::palw_class_ladder_rules_for_court_v1(&profile, Some(kary), bundle.court.max_step_leaf_count()) {
        let cost = adm::derive_court_cost_shaped_v1(&profile, rules.cost_shape);
        println!("cost, dissection court at the RC arity: {cost:?}");
        if let Ok(cost) = &cost {
            println!("  close chunks {}", kaspa_consensus_core::palw_mode_v2::palw_close_chunks_for_bytes_v1(cost.max_close_bytes));
        }
    }
    if let Ok(counted) = counted {
        let collateral = kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_collateral_for_claim_lifetime_v1(counted);
        println!(
            "collateral for a v5 claim lifetime {collateral} sompi; the genesis premine carves {} per bond",
            kaspa_consensus_core::config::premine::GENESIS_BOND_COLLATERAL_SOMPI
        );
    }
    for object in &bundle.genesis_objects {
        if let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
            class_id, pwu_rule, share_permille, ..
        } = object
        {
            println!("shipped genesis row {class_id} share {share_permille} pwu {pwu_rule:?}");
        }
    }
}

/// **The class the genesis registers and the class this build can SUPPLY are one class.**
///
/// Two crates spell the graph-v5 dense row, and they must: `kaspa_consensus_core` cannot read this
/// crate (the dependency runs the other way), so the consensus-side builder derives the width and
/// the canonical job itself while `classes::a16_graph_v5_row_v1` derives them beside the artifact
/// shape a file must match. Nothing in either crate forces the two to agree — this test is what
/// does, and it is the falsifier the width constant's doc names.
///
/// A disagreement is not cosmetic: `n_ctx` is inside `shape_profile_id`, so a moved width is a
/// class the chain registered and no node can resolve (`resolve_class_v1` answers by class id), and
/// a moved canonical job is a different `pwu_per_inference` under one class id — the n_ctx 17
/// BURNED failure exactly.
#[test]
fn the_genesis_row_and_the_shipped_row_are_one_class() {
    let shipped = misaka_palw_base0::classes::a16_graph_v5_row_v1().expect("this build tables the graph-v5 row");
    let genesis = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_graph_v5_profile_v1().expect("the genesis row projects");
    assert_eq!(
        misaka_palw_base0::classes::A16_CONVERTER_ROTARY_SPAN_V1,
        kaspa_consensus_core::palw_qwen25_profile::QWEN25_A16_GRAPH_V5_N_CTX,
        "the two crates project the row at different widths, which is two classes"
    );
    assert_eq!(
        shipped.class_id(),
        genesis.shape_profile_id(),
        "the class the genesis registers is not the class this build can supply"
    );
    assert_eq!(
        shipped.canonical_job,
        kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_graph_v5_canonical_v1(),
        "one class id, two canonical jobs — the registration would be paid per a job the producer does not run"
    );
    println!("graph-v5 dense row, one class in two crates: {} job {:?}", shipped.class_id(), shipped.canonical_job);
}

/// **The row the GENESIS registers is admissible under the ruleset that registers it** — J's
/// admission test, run over the genesis object rather than over one the test built.
///
/// The difference matters. J's test constructs a registration and proves the ROW can be admitted;
/// this one takes the `ClassRegistered` out of `palw_rc_shipped_params()`'s own genesis set — the
/// carriage, the pwu rule, the artifact root and the share the chain will actually carry — and puts
/// that object through `verify_class_admission_v5` under the court the shipped ruleset derives. A
/// genesis object is verified against the committed catalog rather than through this gate, so
/// nothing on the boot path would have said whether the class the network mints could also have
/// walked in through the door every later class uses.
///
/// **And the answer is "every layer but one", which is a finding and not a pass.** The row as
/// registered — 489‰ — is refused by `NotEndToEndCertified`: the RC's pinned certified set gives
/// the `PALW-QWEN25-A16` family the kernel set of the graph-**v2** profile, which does not reach
/// `KDESC_A16_ATTN_FUSED`, so no certified family covers this class's kernels. That is ADR-0082
/// stream I's end-to-end court drill and the `PALW_RC_COURT_E2E_ROOT_BYTES` re-pin that records it
/// (5f card §6 calls the drill "the gate, and admission is not"). Until it lands the class is
/// registered, carries cadence, and is weightless under `palw_uncertified_weightless` — which is
/// exactly what this test says, in the refusal's own words, so the day the drill lands the first
/// assertion goes green by measurement.
#[test]
fn the_genesis_registered_row_is_admissible_under_the_armed_rc_court() {
    use kaspa_consensus_core::palw_class_admission_v2 as adm;
    use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;

    let params = kaspa_consensus_core::config::params::palw_rc_shipped_params();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        panic!("the RC preset is not a v2 bundle");
    };
    let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_graph_v5_profile_v1().expect("projects");
    let class_id = profile.shape_profile_id();
    let registration = bundle
        .genesis_objects
        .iter()
        .find(|o| matches!(o, PalwConsensusObjectV2::ClassRegistered { class_id: id, .. } if *id == class_id))
        .expect("the shipped genesis registers the graph-v5 row")
        .clone();
    let PalwConsensusObjectV2::ClassRegistered { admission: Some(carriage), share_permille, .. } = &registration else {
        panic!("the genesis row carries no profile, so the fold cannot know it is fused");
    };
    let canonical = carriage.canonical.clone();
    let genesis_share = *share_permille;

    // The court this ruleset PLAYS: the fence is armed, so the arity is the derived one.
    let played = kaspa_consensus_core::palw_court_v2::palw_court_params_at_v2(bundle, params.palw_kary_court_active_at(0))
        .expect("the armed court derives an arity");
    let kary = adm::PalwKaryCourtV1 {
        dissection_arity: played.dissection_arity(),
        prompt_ids_form: params.palw_prompt_ids_form_at(0),
        window_court_daa: bundle.state.window_court(),
    };
    let rules = kaspa_consensus_core::palw_context_ladder::palw_class_ladder_rules_for_court_v1(
        &profile,
        Some(kary),
        bundle.court.max_step_leaf_count(),
    )
    .expect("a mapped class has ladder rules");
    // **The certified set is the NETWORK's, not an empty slice.** A weight-bearing registration is
    // checked against the family set the bundle commits to (ADR-0069 Decision 5), and this row
    // carries the dense tier's share — passing `&[]` would ask the gate about a different network.
    let certified = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1();

    // 1. Every ADR-0082 layer — the carriage, the ladder, the cost, the court — on the object the
    //    genesis carries, with the one question certification answers taken out of the way.
    let weightless = match registration.clone() {
        PalwConsensusObjectV2::ClassRegistered {
            class_id,
            artifact_root,
            slash_value_per_pwu,
            pwu_rule,
            initial_target,
            activation_daa,
            admission,
            ..
        } => PalwConsensusObjectV2::ClassRegistered {
            class_id,
            artifact_root,
            slash_value_per_pwu,
            pwu_rule,
            initial_target,
            share_permille: 0,
            activation_daa,
            admission,
        },
        other => panic!("not a registration: {other:?}"),
    };
    let admitted =
        adm::verify_class_admission_v5(bundle, &profile, &canonical, &weightless, &certified, &certified, Some(rules), Some(kary));
    assert!(
        admitted.is_ok(),
        "the class the genesis mints fails a static layer, not merely certification — ladder {}, arity {}, ids {:?}: {admitted:?}",
        bundle.court.max_step_leaf_count(),
        kary.dissection_arity,
        kary.prompt_ids_form
    );
    let entry = admitted.expect("checked");
    println!(
        "genesis-registered graph-v5 row admitted weightless: close {} B = {} chunk(s), canonical {} leaves, worst {}, arity {}",
        entry.court_cost.max_close_bytes,
        kaspa_consensus_core::palw_mode_v2::palw_close_chunks_for_bytes_v1(entry.court_cost.max_close_bytes),
        entry.canonical_step_leaf_count,
        entry.max_step_leaf_count,
        kary.dissection_arity
    );

    // 2. And with the SHARE the genesis grants — which is the assertion that was red until the
    //    fourth certified family (`PALW-QWEN25-A16-V5`) landed. ADR-0069 Decision 5: weight is the
    //    thing certification buys, and a genesis registration does not pass through this gate, so
    //    without this test the network could grant 489‰ to a class no certified family covers and
    //    nothing on the boot path would say so. The refusal is named in the failure arm because it
    //    is the one this will come back as.
    let weighted =
        adm::verify_class_admission_v5(bundle, &profile, &canonical, &registration, &certified, &certified, Some(rules), Some(kary));
    match &weighted {
        Ok(_) => println!("graph-v5 dense @ 512 holds its genesis {genesis_share}‰ against a certified family"),
        Err(adm::PalwClassAdmissionError::NotEndToEndCertified { share }) => panic!(
            "the genesis grants {share}‰ to a class no certified family covers — the fused kernel \
             (KDESC_A16_ATTN_FUSED) is not in any family's kernel set, so this class registers, produces and is \
             weightless under palw_uncertified_weightless. Close it with the fused family's drill and the \
             PALW_RC_COURT_E2E_ROOT_BYTES re-pin, or register the row at share 0."
        ),
        Err(other) => panic!("the weight-bearing row failed on something other than certification: {other:?}"),
    }
    assert_eq!(genesis_share, kaspa_consensus_core::config::params::PALW_RC_GENESIS_QWEN25_A16_SHARE_PERMILLE);

    // 3. And WITHOUT the fence the same object is refused BY NAME — the guard the 5f card §6 names,
    //    which is also why the assembly arms the fence rather than leaving the row unprosecutable.
    match adm::verify_class_admission_v5(bundle, &profile, &canonical, &weightless, &certified, &certified, None, None) {
        Err(adm::PalwClassAdmissionError::FusedAttentionNeedsTheKaryCourt) => {}
        other => panic!("an unfenced court must refuse the genesis row by name, got {other:?}"),
    }
}
