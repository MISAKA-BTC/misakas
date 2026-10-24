//! The held-GDN filer against the genesis hybrid's dev fixture (graph-v8, held v5): a producer whose committed checkpoint is
//! locally consistent (the leg and the binding re-committed over a forged state, so every hash the chain holds verifies) is found
//! by an outside bond's own recompute alone, demanded through the held DA court, and accused once it answers — the court's own
//! terminal convicts the accusation; silent, its demand defaults; honest, it is never demanded.
use super::*;
use kaspa_consensus_core::palw_attempt_rules_v1::PalwAttemptRulesV1;
use kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1;
use kaspa_consensus_core::palw_attn_court_v1::{PalwAttnCheckpointAnchorV1, PalwAttnChunkOpeningV1};
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_context_ladder::{palw_checkpoint_leaf_carries_recurrence_v1, palw_checkpoint_positions_at_v1};
use kaspa_consensus_core::palw_held_da_v1::PalwHeldDisclosureCarriageV1;
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_qwen36_profile::{PalwQwen36GeometryV1, qwen36_held_canonical_v1, qwen36_profile_v8};
use kaspa_consensus_core::palw_step_leg::{PALW_STEP_LEG_MAX_LEAVES, binding_commitment_root_v1, checkpoint_leaf_hash_v2, step_opening_capped_v1};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use misaka_palw_base0::qwen36_backend::Qwen36Backend;
use std::sync::Arc;

const NETWORK: &[u8] = b"misaka-palw-rc";

pub(crate) fn geometry() -> PalwQwen36GeometryV1 {
    PalwQwen36GeometryV1 {
        layer_count: 4,
        full_attention_interval: 4,
        hidden_dim: 32,
        attn_heads: 4,
        attn_kv_heads: 2,
        attn_head_dim: 16,
        rope_dims: 4,
        rope_freq_base_bits: 0x4B18_9680,
        gdn_k_heads: 2,
        gdn_v_heads: 4,
        gdn_head_dim: 8,
        gdn_conv_kernel: 4,
        n_experts: 8,
        experts_per_token: 4,
        moe_dim: 16,
        shared_dim: 16,
        attn_output_gate: 1,
        vocab_size: 64,
        n_ctx: 160,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 512,
    }
}

fn bond(n: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(n), index: 0 })
}

/// The forged world: the honest view, the forged view (checkpoint `index`'s state with one byte of `(node_slot, head)`'s first
/// demanded chunk flipped, the leg and binding re-committed), the forged state chunks the producer would disclose, and the oracle.
pub(crate) struct World {
    pub(crate) capture: Vec<u8>,
    pub(crate) forged_capture: Vec<u8>,
    pub(crate) profile: kaspa_consensus_core::palw_step::PalwShapeProfileV3,
    pub(crate) backend: Qwen36Backend,
    pub(crate) honest: Base0GdnOwnedMaterialV2,
    pub(crate) forged: Base0GdnOwnedMaterialV2,
    forged_chunks: Vec<Vec<u8>>,
    positions: u32,
    index: u32,
    leg_leaves: Vec<Hash64>,
    pub(crate) shape_profile_id: Hash64,
    pub(crate) openings: Vec<PalwArtifactOpeningV1>,
    pub(crate) oracle: PalwProvenOperandsV1,
    pub(crate) inventory_root: Hash64,
}

pub(crate) fn world(forge_head: u32) -> World {
    world_with_fault(forge_head, false)
}

fn world_with_fault(forge_head: u32, conv_window: bool) -> World {
    let artifact = Arc::new(misaka_palw_base0::qwen36::qwen36_dev_fixture(4, 8));
    let profile = qwen36_profile_v8(geometry()).expect("graph-v8 projects");
    let backend = Qwen36Backend::from_registered_profile(artifact.clone(), NETWORK.to_vec(), profile.clone(), qwen36_held_canonical_v1(160))
        .expect("servable")
        .with_step_ladder_cap(PALW_STEP_LEG_MAX_LEAVES)
        .with_prompt_ids_form(PalwPromptIdsFormV1::MerkleV1)
        .with_attempt_rules(PalwAttemptRulesV1::CoreV1);
    let (canonical, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x6D_6E02)).expect("a job");
    let out = backend.execute(&palw_attempt_job_v1(canonical, true), &prompt).expect("the honest attempt runs");
    // A held class carries no prompt ids in its material (PanelDa): the accuser has the job's public prompt.
    let carried: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let mut honest = backend
        .gdn_accuser_view_v2(&out.material, None)
        .ok()
        .filter(|v| !v.prompt_token_ids.is_empty())
        .unwrap_or_else(|| backend.gdn_accuser_view_v2(&out.material, Some(&carried)).expect("the accuser's view"));
    // The job's prompt commitment is under the form its attempt rules committed it with; the accuser reads it from the chain.
    let committed = honest.binding.job_context.prompt_token_ids_hash;
    honest.prompt_ids_form = [PalwPromptIdsFormV1::MerkleV1, PalwPromptIdsFormV1::Flat]
        .into_iter()
        .find(|f| kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_match_v1(*f, &honest.prompt_token_ids, &committed))
        .expect("the served prompt ids are the job's under one of the two forms");
    let (b, ctx) = (&honest.binding, &honest.binding.job_context);
    let gdn = kernel_semantics_id_v1(if conv_window { KDESC_Q36_SSM_CONV } else { KDESC_Q36_GDN_STEP });
    let node_slot = (0..u32::MAX)
        .take_while(|s| b.shape_profile.resolve_node_slot(*s).is_some())
        .find(|s| b.shape_profile.resolve_node_slot(*s).is_some_and(|(n, l)| l.is_some() && n.kernel_semantics_id == gdn))
        .expect("a recurrence node");
    let (index, positions) = honest
        .checkpoint_leaves
        .iter()
        .map(|leaf| (leaf.checkpoint_index, palw_checkpoint_positions_at_v1(&b.shape_profile, ctx, leaf.covered_decode_call)))
        .find(|(_, p)| palw_checkpoint_leaf_carries_recurrence_v1(&b.shape_profile, *p))
        .expect("a recurrence-carrying checkpoint");
    let demands = if conv_window {
        gdn_conv_window_demands_v2(b, node_slot, forge_head, index)
    } else {
        gdn_transition_demands_v2(b, node_slot, forge_head, index)
    }
    .expect("units");
    let PalwHeldMissingV1::StateChunk { chunk: first, .. } = demands[0] else { panic!("state-chunk units") };
    let mut kernels = backend.gdn_recompute_kernels_v2().unwrap();
    let mut state = misaka_palw_base0::fp_recompute::base0_fp_recompute_state_at_covered_v1(
        &b.shape_profile,
        ctx,
        &honest.prompt_token_ids,
        &honest.output_token_ids,
        honest.checkpoint_leaves[index as usize].covered_decode_call,
        &mut kernels,
        honest.prompt_ids_form,
    )
    .expect("the honest state");
    state.chunks[first as usize][0] ^= 0x01;
    let forged_root = misaka_palw_base0::fp_interval::base0_state_chunks_root_for_v1(&b.shape_profile, positions, &state.chunks).unwrap();
    let mut forged = honest.clone();
    let (ctx_hash, profile_hash, map) = (ctx.context_hash(), forged.binding.checkpoint_profile.profile_hash(), forged.binding.state_chunk_map_id);
    forged.checkpoint_leaves[index as usize].state_chunks_root = forged_root;
    for j in index as usize + 1..forged.checkpoint_leaves.len() {
        forged.checkpoint_leaves[j].prev_checkpoint_leaf_hash =
            checkpoint_leaf_hash_v2(&ctx_hash, &profile_hash, &map, &forged.checkpoint_leaves[j - 1]);
    }
    let leg = misaka_palw_base0::legs::Base0CheckpointCaptureV1::from_leaves_v1(
        ctx,
        &forged.binding.shape_profile,
        &forged.binding.checkpoint_profile,
        &forged.checkpoint_leaves,
    )
    .unwrap();
    forged.binding.checkpoint_merkle_root = leg.merkle_root;
    forged.binding.committed_execution_root = binding_commitment_root_v1(&forged.binding);
    let inventory = misaka_palw_base0::inventory::qwen36_inventory_v1(&artifact, &profile).unwrap();
    let openings: Vec<_> = (0..inventory.operands().len())
        .map(|i| kaspa_consensus_core::palw_artifact::open_artifact_leaf_v1(inventory.operands(), i as u32).unwrap())
        .collect();
    let oracle = PalwProvenOperandsV1::from_openings_v1(&openings, inventory.root()).unwrap();
    let mut retained = misaka_palw_base0::produce::base0_fp_material_decode_v2(&out.material).expect("held folded retention");
    retained.binding = forged.binding.clone();
    retained.checkpoint_leaves = forged.checkpoint_leaves.clone();
    // Held retention omits state chunks; the forged bytes are disclosed through the court below.
    let mut forged_capture = misaka_palw_base0::produce::PALW_BASE0_FP_MATERIAL_MAGIC_V2.to_vec();
    forged_capture.extend(borsh::to_vec(&retained).unwrap());
    World {
        capture: out.material,
        forged_capture,
        profile: profile.clone(),
        backend,
        honest,
        forged,
        forged_chunks: state.chunks,
        positions,
        index,
        leg_leaves: leg.leaf_hashes,
        shape_profile_id: profile.shape_profile_id(),
        openings,
        oracle,
        inventory_root: inventory.root(),
    }
}

pub(crate) fn claim_of(m: &Base0GdnOwnedMaterialV2, w: &World) -> PalwGdnHeldClaimV1 {
    PalwGdnHeldClaimV1 {
        claim: Hash64::from_u64_word(0xC1A1),
        executor: bond(1),
        execution_root: m.binding.committed_execution_root,
        trace_root: Hash64::from_u64_word(0x7A),
        shape_profile_id: w.shape_profile_id,
    }
}

/// The producer's own answer to `unit` (its forged bytes and their path to the committed forged root).
pub(crate) fn producer_answer(w: &World, claim: &PalwGdnHeldClaimV1, unit: PalwHeldMissingV1) -> PalwConsensusObjectV2 {
    let PalwHeldMissingV1::StateChunk { chunk, .. } = unit else { unreachable!() };
    let siblings = kaspa_consensus_core::palw_state_chunk_map::palw_state_chunk_path_for_map_v1(
        &w.forged.binding.shape_profile,
        w.positions,
        &w.forged_chunks,
        chunk,
    )
    .unwrap();
    let opening = step_opening_capped_v1(&w.leg_leaves, u64::from(w.index), PALW_STEP_LEG_MAX_LEAVES).unwrap();
    let anchor = PalwAttnCheckpointAnchorV1 { leaf: w.forged.checkpoint_leaves[w.index as usize].clone(), opening };
    PalwConsensusObjectV2::MaterialDisclosedHeld {
        disclosure: Box::new(PalwHeldDisclosureCarriageV1 {
            version: PALW_HELD_DA_VERSION_V1,
            claim: claim.claim,
            missing: unit,
            binding: w.forged.binding.clone(),
            disclosure: PalwHeldDisclosureV1::StateChunk {
                anchor,
                chunk: PalwAttnChunkOpeningV1 { chunk_index: chunk, chunk_bytes: w.forged_chunks[chunk as usize].clone(), siblings },
            },
            signature: vec![1],
        }),
    }
}

pub(crate) fn sign(_: &[u8], _: &[u8]) -> Option<Vec<u8>> {
    Some(vec![0x51; 8])
}

fn run(w: &World, answering: bool) -> (Vec<PalwHeldMissingV1>, PalwGdnHeldNextV1, PalwGdnHeldCaseV1) {
    run_with_answer_policy(w, |_| answering)
}

fn run_with_answer_policy(
    w: &World,
    mut answer: impl FnMut(Base0GdnCheckpointCourtV2) -> bool,
) -> (Vec<PalwHeldMissingV1>, PalwGdnHeldNextV1, PalwGdnHeldCaseV1) {
    let claim = claim_of(&w.forged, w);
    let kernels = || w.backend.gdn_recompute_kernels_v2().unwrap();
    let mut case = palw_gdn_held_open_v1(claim.clone(), &w.forged.view(), kernels).unwrap().expect("the forged checkpoint is found");
    assert_eq!(case.index, w.index, "the outsider's own recompute names the forged checkpoint");
    let accuser = PalwGdnHeldAccuserV1 {
        bond: bond(2),
        network_domain: Hash64::from_u64_word(0xD0),
        openings: &w.openings,
        oracle: &w.oracle,
        sign: &sign,
    };
    let (mut chain, mut demanded, mut now) = (Vec::new(), Vec::new(), 100u64);
    for _ in 0..256 {
        now += 1;
        match palw_gdn_held_step_v1(&mut case, &w.forged.view(), kernels, &chain, now, 20, &accuser).unwrap() {
            PalwGdnHeldNextV1::Demand { unit, object } => {
                let PalwConsensusObjectV2::DefaultAccusedHeld { accusation } = &object else { panic!("a held demand") };
                kaspa_consensus_core::palw_held_da_v1::palw_held_da_check_accusation_v1(
                    &claim.execution_root,
                    &accusation.missing,
                    &accusation.binding,
                    w.forged.prompt_ids_form,
                )
                .expect("the held DA court admits the demand");
                demanded.push(unit);
                chain.push(object);
                if answer(case.candidates[case.at]) {
                    chain.push(producer_answer(w, &claim, unit));
                }
            }
            PalwGdnHeldNextV1::Wait => {}
            next => return (demanded, next, case),
        }
    }
    panic!("the case did not settle")
}

#[test]
fn rfc06_held_gdn_filer_an_answering_producer_is_accused_and_the_court_convicts() {
    let w = world(0);
    let (demanded, next, case) = run(&w, true);
    let PalwGdnHeldNextV1::Accuse { object } = next else { panic!("an answered forged checkpoint is accused: {next:?}") };
    let PalwConsensusObjectV2::MemoryRoutingV2 { object } = object else { unreachable!() };
    let PalwMemoryRoutingObjectV2::Accused { accusation } = *object else { unreachable!() };
    assert_eq!((accusation.executor_bond, accusation.accuser_bond), (bond(1), bond(2)), "an outside bond accuses the producer");
    let PalwMemoryRoutingEvidenceV2::GdnTransition { refutation } = &accusation.evidence else { panic!("the transition route") };
    let verdict = check_gdn_transition_refutation_v2(refutation, accusation.execution_root, w.shape_profile_id, &w.oracle, PALW_STEP_LEG_MAX_LEAVES)
        .expect("the court's own terminal convicts");
    println!("rfc06 held GDN filer: {} unit(s) demanded and answered, candidate {}, verdict {:?}", demanded.len(), case.at, verdict.mismatch);
}

#[test]
fn rfc06_held_gdn_filer_moves_past_a_candidate_whose_answers_are_honest() {
    // The forged byte sits in head 1's slice: head 0's answers are the honest bytes, so its terminal finds nothing and the filer
    // demands head 1's units next.
    let w = world(1);
    let (demanded, next, case) = run(&w, true);
    assert!(matches!(next, PalwGdnHeldNextV1::Accuse { .. }), "the forged head is reached: {next:?}");
    assert!(case.at >= 1, "the filer moved past the honest candidate (at {})", case.at);
    println!("rfc06 held GDN filer (head 1 forged): {} unit(s) demanded before the accusation", demanded.len());
}

#[test]
fn rfc06_held_gdn_filer_a_silent_producer_is_left_to_the_da_default() {
    let w = world(0);
    let (demanded, next, _) = run(&w, false);
    assert_eq!(demanded.len(), 1, "one demand is open at a time");
    assert!(matches!(next, PalwGdnHeldNextV1::DefaultDue { .. }), "silence past the window is the held DA court's default: {next:?}");
}

#[test]
fn rfc06_held_gdn_filer_an_honest_producer_is_never_demanded() {
    let w = world(0);
    let claim = claim_of(&w.honest, &w);
    let kernels = || w.backend.gdn_recompute_kernels_v2().unwrap();
    assert!(palw_gdn_held_open_v1(claim, &w.honest.view(), kernels).unwrap().is_none(), "no forged checkpoint, no case");
}

/// The node's seam over the forged world: one accusable claim, its public served material, the chain's objects so far.
struct Host<'a> {
    w: &'a World,
    material: Base0GdnOwnedMaterialV2,
    chain: std::cell::RefCell<Vec<PalwConsensusObjectV2>>,
}

impl PalwGdnHeldHostV1 for Host<'_> {
    fn accusable_claims(&self) -> Vec<PalwGdnHeldClaimV1> {
        vec![claim_of(&self.material, self.w)]
    }
    fn material(&self, _: &PalwGdnHeldClaimV1) -> Result<kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<Base0GdnOwnedMaterialV2>, String> {
        kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1::try_from_clone_v1(&self.material).map_err(|e|e.to_string())
    }
    fn kernels(&self, _: &PalwGdnHeldClaimV1) -> Result<Box<dyn Base0FpRecomputeKernelsV1 + '_>, String> {
        Ok(Box::new(self.w.backend.gdn_recompute_kernels_v2()?))
    }
    fn objects(&self, _: &PalwGdnHeldClaimV1) -> Vec<PalwConsensusObjectV2> {
        self.chain.borrow().clone()
    }
    fn openings(&self, _: &PalwGdnHeldClaimV1) -> Result<GdnInventoryOracleV2, String> {
        misaka_palw_base0::GdnInventoryOpeningsV2::try_clone_from_v2(&self.w.openings)?.into_oracle_v2(self.w.inventory_root)
    }
    fn disclose_window_daa(&self) -> u64 {
        20
    }
    fn bond(&self) -> PalwBondKeyV2 {
        bond(2)
    }
    fn network_domain(&self) -> Hash64 {
        Hash64::from_u64_word(0xD0)
    }
    fn sign(&self, m: &[u8], c: &[u8]) -> Option<Vec<u8>> {
        sign(m, c)
    }
}

/// The tick as the node runs it: every object it returns is carried (lands on the chain); an answering producer answers each demand.
fn tick_until_settled(w: &World, material: Base0GdnOwnedMaterialV2, answering: bool) -> (PalwGdnHeldBookV1, Vec<PalwConsensusObjectV2>) {
    let host = Host { w, material, chain: Default::default() };
    let claim = claim_of(&host.material, w);
    let mut book = PalwGdnHeldBookV1::default();
    let mut carried = Vec::new();
    for now in 100..400u64 {
        for (_, object) in palw_gdn_held_filer_tick_v1(&host, &mut book, now) {
            if let PalwConsensusObjectV2::DefaultAccusedHeld { accusation } = &object
                && answering
            {
                host.chain.borrow_mut().push(producer_answer(w, &claim, accusation.missing));
            }
            host.chain.borrow_mut().push(object.clone());
            carried.push(object);
        }
        if book.settled.contains_key(&claim.claim) {
            break;
        }
    }
    (book, carried)
}

#[test]
fn rfc06_held_gdn_filer_tick_files_demands_then_the_accusation_and_settles() {
    let w = world(0);
    let (book, carried) = tick_until_settled(&w, w.forged.clone(), true);
    let claim = Hash64::from_u64_word(0xC1A1);
    assert_eq!(book.settled.get(&claim), Some(&"accused"));
    let accusation = carried.iter().rev().find_map(|o| match o {
        PalwConsensusObjectV2::MemoryRoutingV2 { object } => match object.as_ref() {
            PalwMemoryRoutingObjectV2::Accused { accusation } => Some(accusation.clone()),
            _ => None,
        },
        _ => None,
    });
    let accusation = accusation.expect("the tick carried the accusation");
    let PalwMemoryRoutingEvidenceV2::GdnTransition { refutation } = &accusation.evidence else { panic!("the transition route") };
    check_gdn_transition_refutation_v2(refutation, accusation.execution_root, w.shape_profile_id, &w.oracle, PALW_STEP_LEG_MAX_LEAVES)
        .expect("the carried accusation convicts at the court's own terminal");
    println!("rfc06 held GDN filer tick: {} object(s) carried (demands then the accusation)", carried.len());
}

#[test]
fn rfc06_held_gdn_filer_tick_a_silent_producer_defaults_and_an_honest_one_is_never_touched() {
    let w = world(0);
    let (book, carried) = tick_until_settled(&w, w.forged.clone(), false);
    assert_eq!(book.settled.get(&Hash64::from_u64_word(0xC1A1)), Some(&"defaulted"));
    assert_eq!(carried.len(), 1, "one demand, then the DA-7 default is the fold's");
    let (book, carried) = tick_until_settled(&w, w.honest.clone(), true);
    assert_eq!(book.settled.get(&Hash64::from_u64_word(0xC1A1)), Some(&"honest"));
    assert!(carried.is_empty(), "an honest producer is never demanded");
}

#[test]
fn conv_window_only_forgery_is_demanded_and_convicted_by_its_own_terminal() {
    for head in [0, 1, 3] {
        let w = world_with_fault(head, true);
        let (demands, next, case) = run(&w, true);
        let PalwGdnHeldNextV1::Accuse { object } = next else {
            panic!("window fault not reached: {next:?}");
        };
        let PalwConsensusObjectV2::MemoryRoutingV2 { object } = object else { unreachable!() };
        let PalwMemoryRoutingObjectV2::Accused { accusation } = *object else { unreachable!() };
        assert!(accusation.artifact_openings.is_empty(), "the window terminal reads no artifact operands");
        let claim = kaspa_consensus_core::palw_shard_court_v1::PalwOneMoveClaimV2 {
            execution_root: accusation.execution_root,
            class_id: w.shape_profile_id,
            artifact_root: w.inventory_root,
        };
        assert_eq!(
            kaspa_consensus_core::palw_memory_routing_v2::palw_memory_routing_verdict_v2(
                &accusation,
                &claim,
                PALW_STEP_LEG_MAX_LEAVES,
                None,
                0
            )
            .unwrap(),
            kaspa_consensus_core::palw_shard_court_v1::PalwShardCourtVerdictV1::ExecutorGuilty
        );
        let PalwMemoryRoutingEvidenceV2::GdnConvWindow { refutation } = accusation.evidence else {
            panic!("window terminal expected");
        };
        check_gdn_conv_window_refutation_v2(&refutation, accusation.execution_root, w.shape_profile_id, PALW_STEP_LEG_MAX_LEAVES)
            .expect("the real window terminal convicts producer-authenticated bytes");
        assert!(case.candidates[..case.at].iter().any(|c| matches!(c, Base0GdnCheckpointCourtV2::Transition { .. })));
        assert_eq!(
            case.demands,
            gdn_conv_window_demands_v2(&w.forged.binding, refutation.conv_slot, refutation.head, w.index).unwrap()
        );
        println!("conv-only fault head {head}: {} held units observed before window accusation", demands.len());
    }
}

#[test]
fn silent_conv_window_after_honest_recurrence_answers_uses_the_existing_da_default() {
    let w = world_with_fault(0, true);
    let (_, next, case) = run_with_answer_policy(&w, |candidate| matches!(candidate, Base0GdnCheckpointCourtV2::Transition { .. }));
    assert!(matches!(next, PalwGdnHeldNextV1::DefaultDue { .. }));
    assert!(matches!(case.candidates[case.at], Base0GdnCheckpointCourtV2::ConvWindow { .. }));
}

#[test]
fn a_refused_checkpoint_terminal_is_not_an_honest_candidate() {
    assert_eq!(terminal_found_fault_v1(Ok(())).unwrap(), true);
    assert_eq!(terminal_found_fault_v1::<()>(Err(PalwStepRefuteError::NoFaultFound.into())).unwrap(), false);
    assert!(terminal_found_fault_v1::<()>(Err(PalwStepRefuteError::Unadjudicable.into())).is_err());
    let quota = kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<u8>("test quota", "refused");
    assert!(terminal_found_fault_v1::<()>(Err(PalwGdnCourtErrorV2::Resource(quota))).is_err());
}

#[test]
fn refused_next_candidate_preserves_the_current_candidate_and_answers_for_retry() {
    let w = world(1);
    let mut case = palw_gdn_held_open_v1(claim_of(&w.forged, &w), &w.forged.view(), || w.backend.gdn_recompute_kernels_v2().unwrap())
        .unwrap()
        .unwrap();
    let accuser = PalwGdnHeldAccuserV1 {
        bond: bond(2), network_domain: Hash64::from_u64_word(0xD0), openings: &w.openings, oracle: &w.oracle, sign: &sign,
    };
    assert!(matches!(palw_gdn_held_step_v1(&mut case, &w.forged.view(),
        || w.backend.gdn_recompute_kernels_v2().unwrap(), &[], 100, 20, &accuser).unwrap(), PalwGdnHeldNextV1::Demand { .. }));
    let chain: Vec<_> = case.demands.iter().map(|unit| producer_answer(&w, &case.claim, *unit)).collect();
    note_answers_v1(&mut case, &w.forged.view(), &chain).unwrap();
    assert!(case.answers.iter().all(Option::is_some), "retain authenticated answers, not merely empty slots");
    assert!(case.demanded.is_some());
    let before = (case.at, case.demands.clone(), borsh::to_vec(&case.answers).unwrap(), case.demanded);
    let mut unavailable = w.forged.view();
    unavailable.prompt_token_ids = &[];
    let mut kernels = || w.backend.gdn_recompute_kernels_v2().unwrap();
    assert!(load_candidate_v1(&mut case, 1, &unavailable, &mut kernels).is_err());
    assert_eq!((case.at, case.demands.clone(), borsh::to_vec(&case.answers).unwrap(), case.demanded), before);
    load_candidate_v1(&mut case, 1, &w.forged.view(), &mut kernels).expect("the restored material can be retried");
    assert_eq!(case.at, 1);
}

#[path = "palw_gdn_held_action_owner_tests_v2.rs"]
mod action_owner_tests;

#[path = "palw_gdn_held_answer_owner_tests_v3.rs"]
mod answer_owner_tests;

#[path = "palw_gdn_held_geometry_owner_tests_v4.rs"]
mod geometry_owner_tests;

#[path = "palw_gdn_held_leaf_leg_tests_v3.rs"]
mod leaf_leg_tests;

#[path = "palw_gdn_held_court_resource_tests_v3.rs"]
mod court_resource_tests;

#[path = "palw_gdn_held_witness_owner_tests_v4.rs"]
mod witness_owner_tests;

#[path = "palw_gdn_held_conv_owner_tests_v3.rs"]
mod conv_owner_tests;
