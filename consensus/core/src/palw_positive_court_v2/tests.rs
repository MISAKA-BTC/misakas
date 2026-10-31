use super::gate::{ClaimFormV2, NATIVE_CREDIT_CONSUMERS_V2, NativeCreditDenialV2, positive_native_credit_v2};
use super::service::*;
use super::*;
use crate::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use crate::palw_step_leg::{step_merkle_root_v1, step_opening_v1};
use crate::palw_tir_class_v1::PalwTirClassV1;
use crate::palw_tir_court_v1::test_support::{TinyExecution, tiny_execution};

fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}
pub(super) fn policy() -> ExactPolicyV2 {
    ExactPolicyV2 {
        max_leaves: 1 << 20,
        max_part_bytes: 1 << 20,
        max_part_heap: 8 << 20,
        max_total_bytes: 64 << 20,
        max_elements_per_leaf: 1_000_000,
        max_terms_per_leaf: 1_000_000,
        max_total_elements: 1 << 40,
        max_total_terms: 1 << 40,
        max_commit_bytes: 1 << 20,
        deadline: 100,
    }
}
pub(super) fn result(x: &TinyExecution) -> MinerResultV2 {
    MinerResultV2 {
        version: 2,
        network: h(101),
        branch: h(102),
        ruleset: h(103),
        claim: h(104),
        job: x.binding.job_context.clone(),
        artifact: x.binding.artifact_root,
        output: crate::palw_attempt_rules_v1::palw_attempt_output_root_v1(&x.binding.job_context, &x.generated),
        sampling: crate::palw_decode_select_v2::PalwDecodeSamplingV2::GREEDY,
        service_policy: h(105),
        dependencies: h(106),
    }
}
pub(super) struct Signed {
    pub(super) result: MinerResultV2,
    pub(super) witness: PanelWitnessV2,
    pub(super) policy: ExactPolicyV2,
    pub(super) miner_signature: SignatureV2,
    pub(super) miner_key: Vec<u8>,
    pub(super) panel_signature: SignatureV2,
    pub(super) panel_key: Vec<u8>,
}
impl Signed {
    pub(super) fn new(x: &TinyExecution, result: MinerResultV2, policy: ExactPolicyV2) -> Self {
        let witness = PanelWitnessV2 {
            version: 2,
            result: result.id(),
            policy: policy.id(),
            attempt: 0,
            binding: x.binding.clone(),
            generated: x.generated.clone(),
        };
        let miner = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([31; 32]);
        let panel = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([32; 32]);
        let miner_signature = SignatureV2(
            libcrux_ml_dsa::ml_dsa_87::sign(&miner.signing_key, result.id().as_byte_slice(), RESULT_CONTEXT, [33; 32])
                .unwrap()
                .as_ref()
                .to_vec(),
        );
        let panel_signature = SignatureV2(
            libcrux_ml_dsa::ml_dsa_87::sign(&panel.signing_key, witness.id().as_byte_slice(), WITNESS_CONTEXT, [34; 32])
                .unwrap()
                .as_ref()
                .to_vec(),
        );
        Self {
            result,
            witness,
            policy,
            miner_signature,
            miner_key: miner.verification_key.as_ref().to_vec(),
            panel_signature,
            panel_key: panel.verification_key.as_ref().to_vec(),
        }
    }
    pub(super) fn admit(&self, registered: &PalwTirClassV1) -> Result<ExactCaseV2, ErrorV2> {
        ExactCaseV2::admit(
            self.result.clone(),
            &self.miner_signature,
            &self.miner_key,
            self.witness.clone(),
            &self.panel_signature,
            &self.panel_key,
            registered,
            self.policy,
            PalwPromptIdsFormV1::Flat,
            1,
            AdmissionScopeV2 {
                network: h(101),
                branch: h(102),
                ruleset: h(103),
                required_form: ClaimFormV2::MinerResultV2,
                registered_class_id: self.result.job.shape_profile_id,
            },
        )
    }
}
pub(super) type EncodedPart = (Vec<u8>, Option<Vec<u8>>);
pub(super) fn parts(x: &TinyExecution, policy: ExactPolicyV2) -> Vec<EncodedPart> {
    let space = PalwTirStepSpaceV1::new(&x.binding.class).unwrap();
    let rules = policy.rules(PalwPromptIdsFormV1::Flat);
    (0..x.binding.step_leaf_count)
        .map(|i| {
            let cone = build_tir_cone_refutation_v1(&x.binding, i, x, &rules).unwrap();
            let leaf = space.leaf_at(&x.binding.job_context, i).unwrap();
            let trace = match leaf.kind {
                PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. }
                    if occurrence as usize == space.occurrences().len() - 1 && node == space.program.logits =>
                {
                    let row = leaf.position + 1 - x.binding.job_context.declared_prefill_tokens;
                    Some(
                        tir_logits_event_disclosure_v1(
                            &x.binding,
                            &x.rows,
                            &x.generated,
                            row,
                            (first_element / crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u64) as u8,
                        )
                        .unwrap(),
                    )
                }
                _ => None,
            };
            (borsh::to_vec(&cone).unwrap(), trace.map(|p| borsh::to_vec(&p).unwrap()))
        })
        .collect()
}
fn submit(case: &mut ExactCaseV2, part: &EncodedPart) -> Result<(), ErrorV2> {
    case.submit_encoded(&part.0, part.1.as_deref(), 2)
}

#[test]
fn cold_node_checks_every_relation_and_full_vocabulary_before_passing() {
    let x = tiny_execution(None);
    let signed = Signed::new(&x, result(&x), policy());
    let journal = parts(&x, signed.policy);
    let mut node = signed.admit(&x.binding.class).unwrap();
    // The receiving Node has no execution store, model tensors, or input provider.
    drop(x);
    assert_eq!(node.finish(2), Err(ErrorV2::Incomplete));
    for (i, part) in journal.iter().enumerate() {
        submit(&mut node, part).unwrap();
        assert_eq!(node.cursor(), i as u64 + 1);
        assert_eq!(node.statuses(), (ClaimStatusV2::Checking, WitnessStatusV2::Checking));
    }
    assert_eq!(node.finish(3), Ok(ClaimStatusV2::Passed));
    assert_eq!(node.statuses(), (ClaimStatusV2::Passed, WitnessStatusV2::Complete));
    assert_eq!(node.finish(3), Err(ErrorV2::Closed));
}

#[test]
fn last_step_omission_cannot_pass_and_expires_unverified() {
    let x = tiny_execution(None);
    let signed = Signed::new(&x, result(&x), policy());
    let mut node = signed.admit(&x.binding.class).unwrap();
    let journal = parts(&x, signed.policy);
    for part in &journal[..journal.len() - 1] {
        submit(&mut node, part).unwrap();
    }
    assert_eq!(node.finish(3), Err(ErrorV2::Incomplete));
    node.expire(100);
    assert_eq!(node.statuses(), (ClaimStatusV2::EndedUnverified, WitnessStatusV2::Incomplete));
    assert_eq!(submit(&mut node, journal.last().unwrap()), Err(ErrorV2::Closed));
}

#[test]
fn duplicate_or_skipped_leaf_rejects_panel_without_convicting_miner() {
    let x = tiny_execution(None);
    let signed = Signed::new(&x, result(&x), policy());
    let journal = parts(&x, signed.policy);
    let mut duplicate = signed.admit(&x.binding.class).unwrap();
    submit(&mut duplicate, &journal[0]).unwrap();
    assert_eq!(submit(&mut duplicate, &journal[0]), Err(ErrorV2::Order));
    assert_eq!(duplicate.statuses(), (ClaimStatusV2::Checking, WitnessStatusV2::Rejected));
    let mut skipped = signed.admit(&x.binding.class).unwrap();
    assert_eq!(submit(&mut skipped, &journal[1]), Err(ErrorV2::Order));
    assert_eq!(skipped.cursor(), 0);
    assert_eq!(skipped.statuses(), (ClaimStatusV2::Checking, WitnessStatusV2::Rejected));
}

#[test]
fn panel_committed_false_intermediate_is_never_a_miner_conviction() {
    let honest = tiny_execution(None);
    let forged = tiny_execution(Some((7, 1)));
    let signed = Signed::new(&forged, result(&honest), policy());
    let mut node = signed.admit(&honest.binding.class).unwrap();
    let journal = parts(&forged, signed.policy);
    let mut rejected = false;
    for part in &journal {
        if submit(&mut node, part).is_err() {
            rejected = true;
            break;
        }
    }
    assert!(rejected, "the Panel's self-consistent commitment cannot hide a false relation");
    assert_eq!(node.statuses(), (ClaimStatusV2::Checking, WitnessStatusV2::Rejected));
    node.expire(100);
    assert_eq!(node.statuses(), (ClaimStatusV2::EndedUnverified, WitnessStatusV2::Rejected));
}

#[test]
fn wrong_miner_output_is_refuted_only_after_complete_independent_check() {
    let x = tiny_execution(None);
    let mut bad = result(&x);
    bad.output = h(900);
    let signed = Signed::new(&x, bad, policy());
    let mut node = signed.admit(&x.binding.class).unwrap();
    let journal = parts(&x, signed.policy);
    for part in &journal {
        submit(&mut node, part).unwrap();
    }
    assert_eq!(node.statuses().0, ClaimStatusV2::Checking);
    assert_eq!(node.finish(3), Ok(ClaimStatusV2::Refuted));
    assert_eq!(node.statuses().1, WitnessStatusV2::Complete);
}

#[test]
fn all_signers_agreeing_to_wrong_decode_still_cannot_pass() {
    let x = crate::palw_tir_court_v1::test_support::tiny_execution_selecting_in(
        |_, _, _| tiny_execution(None).binding.job_context,
        None,
        &|_, row, _| ((crate::palw_step_refute::base0_decode_token_select_v1(row) + 1) % row.len()) as u32,
    );
    let signed = Signed::new(&x, result(&x), policy());
    let journal = parts(&x, signed.policy);
    let mut node = signed.admit(&x.binding.class).unwrap();
    assert!(journal.iter().any(|p| submit(&mut node, p).is_err()));
    assert_eq!(node.statuses(), (ClaimStatusV2::Checking, WitnessStatusV2::Rejected));
}

#[test]
fn missing_logits_bridge_is_not_treated_as_arithmetic_acquittal() {
    let x = tiny_execution(None);
    let signed = Signed::new(&x, result(&x), policy());
    let mut node = signed.admit(&x.binding.class).unwrap();
    let mut journal = parts(&x, signed.policy);
    let position = journal.iter().position(|p| p.1.is_some()).unwrap();
    journal[position].1 = None;
    for part in &journal[..position] {
        submit(&mut node, part).unwrap();
    }
    assert_eq!(submit(&mut node, &journal[position]), Err(ErrorV2::Incomplete));
    assert_eq!(node.statuses(), (ClaimStatusV2::Checking, WitnessStatusV2::Rejected));
}

#[test]
fn foreign_trace_binding_and_altered_trace_lane_are_rejected() {
    let x = tiny_execution(None);
    let signed = Signed::new(&x, result(&x), policy());
    let journal = parts(&x, signed.policy);
    let position = journal.iter().position(|p| p.1.is_some()).unwrap();
    for binding_tamper in [true, false] {
        let mut node = signed.admit(&x.binding.class).unwrap();
        for part in &journal[..position] {
            submit(&mut node, part).unwrap();
        }
        let mut trace: PalwTirTraceEventDisclosureV1 = borsh::from_slice(journal[position].1.as_ref().unwrap()).unwrap();
        let PalwTirTraceEventDisclosureV1::Tiled { binding, tile_lanes, .. } = &mut trace else { panic!("tiled fixture") };
        if binding_tamper {
            binding.artifact_root = h(999);
        } else {
            tile_lanes[0] ^= 1;
        }
        let bad = (journal[position].0.clone(), Some(borsh::to_vec(&trace).unwrap()));
        assert!(submit(&mut node, &bad).is_err());
        assert_eq!(node.statuses(), (ClaimStatusV2::Checking, WitnessStatusV2::Rejected));
    }
}

#[test]
fn swapped_input_and_registered_program_are_rejected_at_admission() {
    let x = tiny_execution(None);
    let mut bad = result(&x);
    bad.job.prompt_token_ids_hash = h(777);
    let input = Signed::new(&x, bad, policy());
    assert!(matches!(input.admit(&x.binding.class), Err(ErrorV2::Binding)));
    let signed = Signed::new(&x, result(&x), policy());
    let mut other = x.binding.class.clone();
    other.tokenizer_id = h(778);
    assert!(matches!(signed.admit(&other), Err(ErrorV2::Binding)));
}

#[test]
fn same_program_with_different_weights_cannot_replace_the_registered_class_root() {
    let honest = tiny_execution(None);
    let mut foreign = tiny_execution(None);
    foreign.ops[0].bytes[0] ^= 1;
    foreign.binding.artifact_root =
        crate::palw_artifact::artifact_root_v1(&foreign.ops.iter().map(crate::palw_artifact::artifact_leaf_v1).collect::<Vec<_>>())
            .unwrap();
    let other_id = foreign.binding.class.class_id(&foreign.binding.artifact_root);
    foreign.binding.job_context.shape_profile_id = other_id;
    let context_hash = foreign.binding.job_context.context_hash();
    foreign.hashes =
        foreign.preimages.iter().map(|p| crate::palw_step_leg::step_tile_leaf_hash_v1(&context_hash, &other_id, p)).collect();
    foreign.binding.step_merkle_root = step_merkle_root_v1(&foreign.hashes).unwrap();
    foreign.binding.full_logits_trace_root =
        crate::palw_step_refute::tiled_logits_trace_root_v1(&foreign.binding.job_context, &foreign.rows, &foreign.generated).unwrap();
    foreign.binding.committed_execution_root = palw_tir_execution_root_v1(
        &context_hash,
        &foreign.binding.full_logits_trace_root,
        &other_id,
        foreign.binding.step_leaf_count,
        &foreign.binding.step_merkle_root,
    );
    // Self-consistency and both signatures do not establish registration.
    verify_tir_binding_v1(&foreign.binding, policy().max_leaves).unwrap();
    let signed = Signed::new(&foreign, result(&foreign), policy());
    assert!(signed.miner_signature.verifies(&signed.miner_key, &signed.result.id(), RESULT_CONTEXT));
    assert_eq!(foreign.binding.class, honest.binding.class, "same code and layout, different artifact");
    let admission = AdmissionScopeV2 {
        network: h(101),
        branch: h(102),
        ruleset: h(103),
        required_form: ClaimFormV2::MinerResultV2,
        registered_class_id: honest.binding.job_context.shape_profile_id,
    };
    assert!(matches!(
        ExactCaseV2::admit(
            signed.result.clone(),
            &signed.miner_signature,
            &signed.miner_key,
            signed.witness.clone(),
            &signed.panel_signature,
            &signed.panel_key,
            &honest.binding.class,
            signed.policy,
            PalwPromptIdsFormV1::Flat,
            1,
            admission
        ),
        Err(ErrorV2::Binding)
    ));
}

#[test]
fn signatures_bind_roles_branch_and_exact_signed_output() {
    let x = tiny_execution(None);
    let signed = Signed::new(&x, result(&x), policy());
    assert!(!signed.miner_signature.verifies(&signed.panel_key, &signed.result.id(), RESULT_CONTEXT));
    assert!(!signed.miner_signature.verifies(&signed.miner_key, &signed.result.id(), WITNESS_CONTEXT));
    let mut result = signed.result.clone();
    result.branch = h(888);
    assert!(!signed.miner_signature.verifies(&signed.miner_key, &result.id(), RESULT_CONTEXT));
    result = signed.result.clone();
    result.output = h(889);
    assert!(!signed.miner_signature.verifies(&signed.miner_key, &result.id(), RESULT_CONTEXT));
    let mut wrong_panel = signed;
    wrong_panel.panel_signature = wrong_panel.miner_signature.clone();
    assert!(matches!(wrong_panel.admit(&x.binding.class), Err(ErrorV2::Authentication)));
}

#[test]
fn correctly_signed_foreign_branch_or_ruleset_cannot_reuse_a_positive_case() {
    let x = tiny_execution(None);
    for field in 0..3 {
        let mut foreign = result(&x);
        match field {
            0 => foreign.network = h(800),
            1 => foreign.branch = h(801),
            _ => foreign.ruleset = h(802),
        }
        // Both actors really sign the foreign scope, so signature verification
        // alone cannot fence current-branch admission.
        let signed = Signed::new(&x, foreign, policy());
        assert!(signed.miner_signature.verifies(&signed.miner_key, &signed.result.id(), RESULT_CONTEXT));
        assert!(matches!(signed.admit(&x.binding.class), Err(ErrorV2::Binding)));
    }
}

#[test]
fn coverage_includes_every_vocabulary_tile_and_ragged_tail() {
    let x = crate::palw_tir_court_v1::test_support::tiny_execution_selecting_in_vocab(
        |class, class_id, prompt| {
            let mut job = tiny_execution(None).binding.job_context;
            job.shape_profile_id = class_id;
            job.tokenizer_id = class.tokenizer_id;
            job.prompt_token_ids_hash = crate::palw_v2::prompt_token_ids_hash_v2(prompt);
            job
        },
        None,
        &|_, row, _| crate::palw_step_refute::base0_decode_token_select_v1(row) as u32,
        8193,
    );
    let signed = Signed::new(&x, result(&x), policy());
    let journal = parts(&x, signed.policy);
    assert_eq!(journal.iter().filter(|x| x.1.is_some()).count(), 6, "three tiles in each of two rows");
    let mut node = signed.admit(&x.binding.class).unwrap();
    for part in &journal {
        submit(&mut node, part).unwrap();
    }
    assert_eq!(node.finish(3), Ok(ClaimStatusV2::Passed));
    let tail = journal.iter().rposition(|x| x.1.is_some()).unwrap();
    let mut incomplete = signed.admit(&x.binding.class).unwrap();
    for part in &journal[..tail] {
        submit(&mut incomplete, part).unwrap();
    }
    assert_eq!(incomplete.finish(3), Err(ErrorV2::Incomplete));
}

#[test]
fn sampled_profile_is_explicitly_refused_until_implemented() {
    let x = tiny_execution(None);
    let mut bad = result(&x);
    bad.sampling.temperature_q = 1;
    let signed = Signed::new(&x, bad, policy());
    assert!(matches!(signed.admit(&x.binding.class), Err(ErrorV2::UnsupportedProfile)));
}

#[test]
fn total_evaluation_budget_is_reserved_across_all_leaves_before_admission() {
    let x = tiny_execution(None);
    let mut small = policy();
    small.max_total_elements = small.max_elements_per_leaf;
    let signed = Signed::new(&x, result(&x), small);
    assert!(matches!(signed.admit(&x.binding.class), Err(ErrorV2::Budget)));
    let mut overflow = policy();
    overflow.max_elements_per_leaf = u64::MAX;
    let signed = Signed::new(&x, result(&x), overflow);
    assert!(matches!(signed.admit(&x.binding.class), Err(ErrorV2::Budget)));
}

#[test]
fn truncated_trailing_and_hostile_vector_lengths_cannot_reach_deserializer() {
    let x = tiny_execution(None);
    let signed = Signed::new(&x, result(&x), policy());
    let journal = parts(&x, signed.policy);
    let mut trailing = journal[0].0.clone();
    trailing.push(0);
    for bytes in [&journal[0].0[..1], &trailing, &[255u8; 16]] {
        let mut node = signed.admit(&x.binding.class).unwrap();
        assert!(node.submit_encoded(bytes, None, 2).is_err());
        assert_eq!(node.cursor(), 0);
        assert_eq!(node.statuses(), (ClaimStatusV2::Checking, WitnessStatusV2::Rejected));
    }
}

#[test]
fn byte_and_heap_caps_are_enforced_without_progress() {
    let x = tiny_execution(None);
    let journal = parts(&x, policy());
    let mut small = policy();
    small.max_part_bytes = journal[0].0.len() as u64 - 1;
    let signed = Signed::new(&x, result(&x), small);
    let mut node = signed.admit(&x.binding.class).unwrap();
    assert_eq!(submit(&mut node, &journal[0]), Err(ErrorV2::Budget));
    assert_eq!(node.cursor(), 0);
    let mut small = policy();
    small.max_part_heap = 1;
    let signed = Signed::new(&x, result(&x), small);
    let mut node = signed.admit(&x.binding.class).unwrap();
    assert_eq!(submit(&mut node, &journal[0]), Err(ErrorV2::Budget));
    assert_eq!(node.statuses(), (ClaimStatusV2::Checking, WitnessStatusV2::Rejected));
}

#[test]
fn authenticated_model_opening_cannot_be_substituted_with_unregistered_weight_bytes() {
    let x = tiny_execution(None);
    let signed = Signed::new(&x, result(&x), policy());
    let journal = parts(&x, signed.policy);
    let (position, mut cone) = journal
        .iter()
        .enumerate()
        .find_map(|(i, p)| {
            let cone: PalwTirConeRefutationV1 = borsh::from_slice(&p.0).unwrap();
            matches!(cone.params, PalwTirParamOpeningV1::Single(_)).then_some((i, cone))
        })
        .unwrap();
    let PalwTirParamOpeningV1::Single(proof) = &mut cone.params else { unreachable!() };
    proof.opened[0].1.bytes[0] ^= 1;
    let mut node = signed.admit(&x.binding.class).unwrap();
    for part in &journal[..position] {
        submit(&mut node, part).unwrap();
    }
    let bad = (borsh::to_vec(&cone).unwrap(), journal[position].1.clone());
    assert!(submit(&mut node, &bad).is_err());
    assert_eq!(node.statuses(), (ClaimStatusV2::Checking, WitnessStatusV2::Rejected));
}

#[test]
fn private_journal_replays_checks_not_pass_flags_and_refuses_trailing_or_early_finish() {
    let x = tiny_execution(None);
    let signed = Signed::new(&x, result(&x), policy());
    let original = signed.admit(&x.binding.class).unwrap();
    let parts = parts(&x, signed.policy);
    let mut journal = Vec::new();
    super::journal::write_header(&original, &mut journal).unwrap();
    let header = journal.clone();
    for part in &parts {
        super::journal::write_part(&mut journal, &part.0, part.1.as_deref(), signed.policy.max_part_bytes).unwrap();
    }
    let no_finish = journal.clone();
    super::journal::write_finish(&mut journal).unwrap();
    let mut restored = signed.admit(&x.binding.class).unwrap();
    assert_eq!(super::journal::replay(&mut restored, &mut &journal[..], 3), Ok(ClaimStatusV2::Passed));
    assert_eq!(restored.cursor(), parts.len() as u64);
    let mut only_parts = signed.admit(&x.binding.class).unwrap();
    assert_eq!(super::journal::replay(&mut only_parts, &mut &no_finish[..], 3), Ok(ClaimStatusV2::Checking));
    let mut trailing = journal.clone();
    trailing.push(2);
    let mut refuse = signed.admit(&x.binding.class).unwrap();
    assert_eq!(super::journal::replay(&mut refuse, &mut &trailing[..], 3), Err(ErrorV2::Wire));
    assert_eq!(refuse.statuses().0, ClaimStatusV2::Checking);
    let mut premature = header.clone();
    super::journal::write_finish(&mut premature).unwrap();
    let mut refuse = signed.admit(&x.binding.class).unwrap();
    assert_eq!(super::journal::replay(&mut refuse, &mut &premature[..], 3), Err(ErrorV2::Incomplete));
    assert_eq!(refuse.cursor(), 0);
    let mut wrong_identity = journal.clone();
    wrong_identity[8] ^= 1;
    let mut refuse = signed.admit(&x.binding.class).unwrap();
    assert_eq!(super::journal::replay(&mut refuse, &mut &wrong_identity[..], 3), Err(ErrorV2::Binding));
    assert_eq!(refuse.cursor(), 0);
    let mut expired = signed.admit(&x.binding.class).unwrap();
    assert_eq!(super::journal::replay(&mut expired, &mut &journal[..], 100), Err(ErrorV2::Deadline));
    assert_eq!(expired.statuses().0, ClaimStatusV2::EndedUnverified);
    let mut truncated = header.clone();
    truncated.push(1);
    truncated.extend_from_slice(&1u32.to_le_bytes());
    let mut refuse = signed.admit(&x.binding.class).unwrap();
    assert_eq!(super::journal::replay(&mut refuse, &mut &truncated[..], 3), Err(ErrorV2::Wire));
    assert_eq!(refuse.cursor(), 0);
    let mut huge = header;
    huge.push(1);
    huge.extend_from_slice(&u32::MAX.to_le_bytes());
    huge.extend_from_slice(&u32::MAX.to_le_bytes());
    let mut refuse = signed.admit(&x.binding.class).unwrap();
    assert_eq!(super::journal::replay(&mut refuse, &mut &huge[..], 3), Err(ErrorV2::Budget));
    assert_eq!(refuse.cursor(), 0);
}

#[test]
fn restart_replays_evidence_and_reorg_revocation_never_restores_pass_flag() {
    let x = tiny_execution(None);
    let signed = Signed::new(&x, result(&x), policy());
    let journal = parts(&x, signed.policy);
    let mut first = signed.admit(&x.binding.class).unwrap();
    for part in &journal {
        submit(&mut first, part).unwrap();
    }
    assert_eq!(first.finish(3), Ok(ClaimStatusV2::Passed));
    first.revoke();
    assert_eq!(first.statuses().0, ClaimStatusV2::Revoked);
    let mut restored = signed.admit(&x.binding.class).unwrap();
    assert_eq!(restored.cursor(), 0);
    assert_eq!(restored.finish(3), Err(ErrorV2::Incomplete));
    for part in &journal {
        submit(&mut restored, part).unwrap();
    }
    assert_eq!(restored.finish(3), Ok(ClaimStatusV2::Passed));
    assert_eq!(restored.charged_bytes(), first.charged_bytes());
}

#[test]
fn passed_private_case_cannot_borrow_native_credit_from_a_colliding_old_claim_id() {
    let x = tiny_execution(None);
    let signed = Signed::new(&x, result(&x), policy());
    let mut node = signed.admit(&x.binding.class).unwrap();
    let scope = AdmissionScopeV2 {
        network: h(101),
        branch: h(102),
        ruleset: h(103),
        required_form: ClaimFormV2::MinerResultV2,
        registered_class_id: x.binding.job_context.shape_profile_id,
    };
    for consumer in NATIVE_CREDIT_CONSUMERS_V2 {
        assert_eq!(
            positive_native_credit_v2(Some(&node), &scope, scope.branch, scope.ruleset, consumer),
            Err(NativeCreditDenialV2::PositiveNotPassed)
        );
    }
    for part in &parts(&x, signed.policy) {
        submit(&mut node, part).unwrap();
    }
    assert_eq!(node.finish(3), Ok(ClaimStatusV2::Passed));
    // No legacy certification row, even one with this same claim ID, can turn
    // this private case into a rooted result binding. The barrier never looks
    // up native Gate C by this freely chosen identifier.
    for consumer in NATIVE_CREDIT_CONSUMERS_V2 {
        assert_eq!(
            positive_native_credit_v2(Some(&node), &scope, scope.branch, scope.ruleset, consumer),
            Err(NativeCreditDenialV2::MissingNativeResultBinding)
        );
        assert_eq!(
            positive_native_credit_v2(Some(&node), &scope, h(909), scope.ruleset, consumer),
            Err(NativeCreditDenialV2::DifferentCurrentBranch)
        );
        assert_eq!(
            positive_native_credit_v2(Some(&node), &scope, scope.branch, h(910), consumer),
            Err(NativeCreditDenialV2::DifferentCurrentRuleset)
        );
    }
    node.revoke();
    for consumer in NATIVE_CREDIT_CONSUMERS_V2 {
        assert_eq!(
            positive_native_credit_v2(Some(&node), &scope, scope.branch, scope.ruleset, consumer),
            Err(NativeCreditDenialV2::PositiveNotPassed)
        );
    }
}

fn service_fixture() -> (ServicePolicyV2, Vec<Vec<u8>>, Vec<Hash64>) {
    let case = h(501);
    let chunks: Vec<_> = (0..4).map(|i| vec![i; CHUNK_BYTES]).collect();
    let hashes = chunks.iter().enumerate().map(|(i, b)| chunk_hash_v2(&case, i as u64, b)).collect::<Vec<_>>();
    (
        ServicePolicyV2 {
            version: 2,
            case,
            source: step_merkle_root_v1(&hashes).unwrap(),
            chunks: 4,
            max_rounds: 3,
            max_unique_bytes: 4 * CHUNK_BYTES as u64,
            max_transmitted_bytes: 8 * (CHUNK_BYTES as u64 + 204),
            work_per_chunk: 10,
            max_work: 80,
            retention_until: 110,
            deadline: 100,
            price_per_chunk: 7,
            hold: 28,
        },
        chunks,
        hashes,
    )
}

#[test]
fn all_panels_share_canonical_overlap_union_and_cannot_create_nonce_budgets() {
    let (p, _, _) = service_fixture();
    let mut ledger = ServiceLedgerV2::new(p.clone(), p.id(), 1).unwrap();
    assert_eq!(ledger.request(WorkRequestV2::Source { first: 0, count: 3 }, 2), Ok(RequestEffectV2::Opened));
    assert_eq!(ledger.request(WorkRequestV2::Source { first: 1, count: 3 }, 2), Ok(RequestEffectV2::Opened));
    assert_eq!(ledger.meter().unique_bytes, 4 * CHUNK_BYTES as u64);
    assert_eq!(ledger.meter().rounds, 2);
    let before = ledger.meter();
    for _ in 0..10 {
        assert_eq!(ledger.request(WorkRequestV2::Source { first: 0, count: 4 }, 2), Ok(RequestEffectV2::Coalesced));
    }
    assert_eq!(ledger.meter(), before);
}

#[test]
fn miner_cannot_be_required_to_construct_witness_trace_or_projection() {
    let (p, _, _) = service_fixture();
    let mut ledger = ServiceLedgerV2::new(p.clone(), p.id(), 1).unwrap();
    let before = ledger.meter();
    for request in [
        WorkRequestV2::BuildWitness,
        WorkRequestV2::ReconstructTrace,
        WorkRequestV2::ComputeProjection,
        WorkRequestV2::ReplayCheckpoint,
    ] {
        assert_eq!(ledger.request(request, 2), Err(ErrorV2::UnsupportedProfile));
        assert_eq!(ledger.meter(), before);
    }
    assert!(!ledger.data_complete());
}

#[test]
fn direct_node_receipt_completes_data_without_panel_ack_and_pays_once() {
    let (p, chunks, hashes) = service_fixture();
    let mut ledger = ServiceLedgerV2::new(p.clone(), p.id(), 1).unwrap();
    ledger.request(WorkRequestV2::Source { first: 0, count: 4 }, 2).unwrap();
    for (i, bytes) in chunks.iter().enumerate() {
        ledger.respond(i as u64, bytes, &step_opening_v1(&hashes, i as u64).unwrap(), 3).unwrap();
    }
    assert!(ledger.data_complete());
    assert_eq!(ledger.meter().paid, 28);
    ledger.respond(0, &chunks[0], &step_opening_v1(&hashes, 0).unwrap(), 3).unwrap();
    assert_eq!(ledger.meter().paid, 28);
    assert_eq!(ledger.meter().transmitted_bytes, 5 * (CHUNK_BYTES as u64 + 204));
    assert!(ledger.conserved());
}

#[test]
fn repeated_invalid_material_spends_aggregate_transport_budget_and_never_completes() {
    let (mut p, chunks, hashes) = service_fixture();
    p.max_transmitted_bytes = 2 * (CHUNK_BYTES as u64 + 204);
    let mut ledger = ServiceLedgerV2::new(p.clone(), p.id(), 1).unwrap();
    ledger.request(WorkRequestV2::Source { first: 0, count: 1 }, 2).unwrap();
    let opening = step_opening_v1(&hashes, 0).unwrap();
    assert_eq!(ledger.respond(0, &chunks[1], &opening, 3), Err(ErrorV2::Binding));
    assert_eq!(ledger.respond(0, &chunks[1], &opening, 3), Err(ErrorV2::Binding));
    assert_eq!(ledger.respond(0, &chunks[0], &opening, 3), Err(ErrorV2::Budget));
    assert_eq!(ledger.meter().paid, 0);
    assert!(!ledger.data_complete());
    assert!(ledger.conserved());
}

#[test]
fn service_opening_depth_is_bounded_before_hashing_and_cannot_complete_delivery() {
    let (p, chunks, hashes) = service_fixture();
    let mut ledger = ServiceLedgerV2::new(p.clone(), p.id(), 1).unwrap();
    ledger.request(WorkRequestV2::Source { first: 0, count: 1 }, 2).unwrap();
    let mut opening = step_opening_v1(&hashes, 0).unwrap();
    opening.siblings.resize(1000, h(1));
    assert_eq!(ledger.respond(0, &chunks[0], &opening, 3), Err(ErrorV2::Budget));
    assert!(!ledger.data_complete());
    assert_eq!(ledger.meter().paid, 0);
    assert!(ledger.conserved());
}

#[test]
fn source_range_round_work_fee_and_retention_limits_fail_without_new_obligation() {
    let (p, chunks, hashes) = service_fixture();
    let mut bad = p.clone();
    bad.retention_until = 99;
    assert!(matches!(ServiceLedgerV2::new(bad.clone(), bad.id(), 1), Err(ErrorV2::Budget)));
    assert!(matches!(ServiceLedgerV2::new(p.clone(), h(999), 1), Err(ErrorV2::Binding)));
    let mut limited = p.clone();
    limited.max_rounds = 1;
    limited.max_work = 10;
    limited.hold = 7;
    let mut ledger = ServiceLedgerV2::new(limited.clone(), limited.id(), 1).unwrap();
    let before = ledger.meter();
    assert_eq!(ledger.request(WorkRequestV2::Source { first: u64::MAX, count: 2 }, 2), Err(ErrorV2::Order));
    assert_eq!(ledger.request(WorkRequestV2::Source { first: 0, count: 2 }, 2), Err(ErrorV2::Budget));
    assert_eq!(ledger.meter(), before);
    ledger.request(WorkRequestV2::Source { first: 0, count: 1 }, 2).unwrap();
    assert_eq!(ledger.request(WorkRequestV2::Source { first: 1, count: 1 }, 2), Err(ErrorV2::Budget));
    let opening = step_opening_v1(&hashes, 0).unwrap();
    ledger.respond(0, &chunks[0], &opening, 3).unwrap();
    assert_eq!(ledger.respond(0, &chunks[0], &opening, 3), Err(ErrorV2::Budget));
    assert_eq!(ledger.meter().paid, 7);
    assert!(ledger.conserved());
}

#[test]
fn delivered_data_and_panel_abandonment_remain_separate_and_refund_is_idempotent() {
    let (p, chunks, hashes) = service_fixture();
    let mut ledger = ServiceLedgerV2::new(p.clone(), p.id(), 1).unwrap();
    ledger.request(WorkRequestV2::Source { first: 0, count: 1 }, 2).unwrap();
    ledger.respond(0, &chunks[0], &step_opening_v1(&hashes, 0).unwrap(), 3).unwrap();
    assert!(ledger.data_complete());
    let x = tiny_execution(None);
    let signed = Signed::new(&x, result(&x), policy());
    let mut node = signed.admit(&x.binding.class).unwrap();
    node.expire(100);
    assert_eq!(node.statuses(), (ClaimStatusV2::EndedUnverified, WitnessStatusV2::Incomplete));
    assert!(ledger.data_complete(), "Panel's incomplete witness cannot erase a Node's data receipt");
    ledger.close();
    let settled = ledger.meter();
    assert_eq!((settled.paid, settled.refunded, settled.remaining), (7, 21, 0));
    ledger.close();
    assert_eq!(ledger.meter(), settled);
    assert!(ledger.conserved());
}
