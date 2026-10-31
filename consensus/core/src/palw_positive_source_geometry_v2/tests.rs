//! Geometry fixtures are data-only. They do not stand in for accepted native
//! requests, source catalogs, reservations, selected history or service receipts.
use super::*;
use kaspa_consensus_core::{
    palw_artifact::{PalwArtifactOperandV1, artifact_leaf_v1, artifact_root_v1, open_artifact_leaf_v1},
    palw_attempt_v2::palw_attempt_job_v1,
    palw_prompt_ids_v1::{prompt_ids_close_bytes_v1, prompt_ids_opening_v1, prompt_token_ids_commitment_v1},
    palw_step_refute::{PALW_LOGITS_TILE_LANES, tiled_logits_scheme_id_v1},
    palw_tir_admission_v1::palw_tir_class_record_v1,
    palw_tir_attempt_v1::{palw_tir_attempt_canonical_v1, palw_tir_attempt_job_for_anchor_of_v1, palw_tir_attempt_job_for_anchor_v1},
    palw_tir_class_v1::PalwTirLayoutV1,
};
use misaka_palw_positive_contract::{
    CONTRACT_REVISION, CONTRACT_VERSION,
    identity::{ChainScope, PrincipalId},
    tir::greedy_selector_id,
};
use misaka_palw_tir::{DType, Ref, TensorType, builder::ProgramBuilder};
fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}
fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn limits() -> GeometryLimitsV2 {
    GeometryLimitsV2 {
        max_program_bytes: misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64,
        max_parameter_instances: 1024,
        max_inventory_leaves: 100_000,
        max_units_per_range: 8,
    }
}
struct Fixture {
    class: PalwTirClassV1,
    record: PalwTirClassRecordV1,
    artifact: Hash64,
    context: PalwJobContextV2,
    result: MinerResultCommit,
    network_form: PalwPromptIdsFormV1,
    prompt: Vec<u32>,
}
impl Fixture {
    fn new(width: u32, prefill: u32, network_form: PalwPromptIdsFormV1, held: bool, artifact: Hash64) -> Self {
        let mut p = ProgramBuilder::new(width, if held { 1 << 21 } else { 1 << 18 });
        let embedding = p.param("embedding", DType::I8, &[width, width], false);
        let mut pre = p.block("pre", vec![]);
        let carry = pre.gather(embedding, Ref::Input(0), 0, 0);
        pre.commit(carry);
        let pre = pre.finish(&[carry]);
        let mut post = p.block("post", vec![TensorType::fixed(DType::I8, &[width])]);
        let logits = post.cast(Ref::CarryIn(0), DType::I32);
        post.commit(logits);
        let Ref::Node(logits) = logits else { unreachable!() };
        let post = post.finish(&[]);
        let mut program = p.finish(pre, vec![], post, logits);
        program.logits_scheme_id = tiled_logits_scheme_id_v1().as_bytes();
        Self::from_program(program, prefill, network_form, artifact)
    }
    fn from_program(program: TirProgramV1, prefill: u32, network_form: PalwPromptIdsFormV1, artifact: Hash64) -> Self {
        let class = PalwTirClassV1 {
            version: PALW_TIR_CLASS_VERSION_V1,
            program: program.encode(),
            tokenizer_id: h(4),
            layout: PalwTirLayoutV1 {
                version: PALW_TIR_LAYOUT_VERSION_V1,
                max_context: 256,
                checkpoint_interval: 2,
                h_tile: 1,
                commit_tiles: program
                    .blocks
                    .iter()
                    .enumerate()
                    .flat_map(|(bi, block)| {
                        let post = program.schedule.post as usize;
                        let logits = program.logits as usize;
                        block
                            .nodes
                            .iter()
                            .enumerate()
                            .filter(|(_, n)| n.commit)
                            .map(move |(ni, _)| if bi == post && ni == logits { PALW_LOGITS_TILE_LANES as u32 } else { 4 })
                    })
                    .collect(),
                state_tiles: vec![],
            },
        };
        let (record, _) = palw_tir_class_record_v1(&class, &artifact).unwrap();
        let prompt: Vec<u32> = (0..prefill).map(|x| x % program.token_bound).collect();
        let context = PalwJobContextV2 {
            version: 2,
            network_id: b"private-a2-geometry".to_vec(),
            job_id: h(5),
            job_nullifier: h(6),
            assignment_id: h(7),
            execution_seed: [8; 32],
            model_profile_id: h(9),
            runtime_manifest_hash: h(10),
            runtime_class_id: h(11),
            shape_profile_id: record.facts.class_id,
            trace_scheme_id: tiled_logits_scheme_id_v1(),
            cu_ruleset_id: h(12),
            tokenizer_id: class.tokenizer_id,
            prompt_token_ids_hash: prompt_token_ids_commitment_v1(record.facts.prompt_ids_form(network_form), &prompt).unwrap(),
            declared_prefill_tokens: prefill,
            exact_decode_tokens: 1,
            max_context_tokens: 257,
        };
        let result = MinerResultCommit {
            version: CONTRACT_VERSION,
            revision: CONTRACT_REVISION,
            scope: ChainScope { network: d(1), branch: d(2), ruleset: d(3) },
            job: Digest(context.job_id.as_bytes()),
            execution: Digest(context.context_hash().as_bytes()),
            miner: PrincipalId(d(13)),
            registered_class: Digest(record.facts.class_id.as_bytes()),
            artifact: Digest(artifact.as_bytes()),
            program: Digest(record.graph_ir_root.as_bytes()),
            input: Digest(context.prompt_token_ids_hash.as_bytes()),
            tokenizer: Digest(context.tokenizer_id.as_bytes()),
            execution_rules: Digest(context.cu_ruleset_id.as_bytes()),
            selector: greedy_selector_id(),
            generation_seed: context.execution_seed,
            temperature_q24: 0,
            prefill,
            decode: 1,
            max_context: 257,
            output: d(14),
            dependencies: d(15),
            service_contract: d(16),
        };
        Self { class, record, artifact, context, result, network_form, prompt }
    }
    fn geometry(&self, cap: GeometryLimitsV2) -> Result<TirSourceGeometryV2, PositiveSourceGeometryErrorV2> {
        TirSourceGeometryV2::derive(&self.class, &self.record, self.artifact, &self.context, &self.result, self.network_form, cap)
    }
    /// Existing canonical producer/draw functions over registered facts. The
    /// resulting DTOs are not an actual native job-origin capability.
    fn use_canonical_attempt(&mut self, prefill_draw: bool) {
        let canonical = palw_tir_attempt_canonical_v1(&self.class).unwrap();
        let anchor = Hash64::from_bytes([84; 64]);
        let (context, prompt) =
            palw_tir_attempt_job_for_anchor_of_v1(&self.record.facts, &anchor, canonical, self.network_form).unwrap();
        assert_eq!(
            palw_tir_attempt_job_for_anchor_v1(&self.class, self.record.facts.class_id, canonical, &anchor, self.network_form),
            Some((context.clone(), prompt.clone()))
        );
        self.context = palw_attempt_job_v1(context, prefill_draw);
        self.prompt = prompt;
        self.result.job = Digest(self.context.job_id.as_bytes());
        self.result.execution = Digest(self.context.context_hash().as_bytes());
        self.result.input = Digest(self.context.prompt_token_ids_hash.as_bytes());
        self.result.tokenizer = Digest(self.context.tokenizer_id.as_bytes());
        self.result.execution_rules = Digest(self.context.cu_ruleset_id.as_bytes());
        self.result.generation_seed = self.context.execution_seed;
        self.result.prefill = self.context.declared_prefill_tokens;
        self.result.decode = self.context.exact_decode_tokens;
        self.result.max_context = self.context.max_context_tokens;
    }
    fn request(&self, kind: SourceKind, first: u64, end: u64) -> SourceRequest {
        SourceRequest {
            version: CONTRACT_VERSION,
            revision: CONTRACT_REVISION,
            case: self.result.case_id(),
            contract: self.result.service_contract,
            source: if kind == SourceKind::ArtifactInventory { self.result.artifact } else { self.result.input },
            kind,
            first,
            end,
        }
    }
}
fn operands() -> Vec<PalwArtifactOperandV1> {
    (0u32..4)
        .map(|row| PalwArtifactOperandV1 {
            tensor_name: "embedding".into(),
            layer: None,
            row_start: row * 4,
            bytes: (row * 4..row * 4 + 4).map(|x| x as u8).collect(),
        })
        .collect()
}
#[test]
fn actual_inventory_opening_and_ragged_prompt_tile_fit_hand_calculated_bounds() {
    let values = operands();
    let artifact = artifact_root_v1(&values.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).unwrap();
    let f = Fixture::new(4, 65, PalwPromptIdsFormV1::MerkleV1, false, artifact);
    let geometry = f.geometry(limits()).unwrap();
    assert_eq!(geometry.source_units(SourceKind::ArtifactInventory), 4);
    assert_eq!(geometry.source_units(SourceKind::Input), 3);
    let opening = open_artifact_leaf_v1(&values, 0).unwrap();
    let bound = geometry.check_unit(SourceKind::ArtifactInventory, 0, &StoredUnitV2::Artifact(opening.clone())).unwrap();
    // operand: 4+9+1+4+4+4; opening index/count/path-prefix: 12; two 64-byte siblings.
    assert_eq!(bound.max_unit_bytes, 166);
    assert_eq!(bound.max_data_bytes, 4);
    assert_eq!(borsh::object_length(&opening).unwrap() as u64, bound.max_unit_bytes);
    let tile = prompt_ids_opening_v1(&f.prompt, 64).unwrap();
    let tail = geometry.check_unit(SourceKind::Input, 2, &StoredUnitV2::Input(tile.clone())).unwrap();
    assert_eq!(tail.max_data_bytes, 4);
    assert_eq!(tail.max_unit_bytes, PALW_PROMPT_IDS_OPENING_HEADER_BYTES + 4 + 2 * 64);
    assert!(borsh::object_length(&tile).unwrap() as u64 <= tail.max_unit_bytes);
    let request = f.request(SourceKind::Input, 0, 3);
    let maximum = geometry.request_bound(&request).unwrap();
    assert_eq!(maximum.max_data_bytes, 128);
    assert_eq!(maximum.max_unit_bytes, prompt_ids_close_bytes_v1(PalwPromptIdsFormV1::MerkleV1, 65).unwrap());
}

#[test]
fn canonical_attempt_zero_context_tokenizer_keeps_nonzero_registered_identity_and_existing_g14_geometry() {
    let values = operands();
    let artifact = artifact_root_v1(&values.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).unwrap();
    for form in [PalwPromptIdsFormV1::Flat, PalwPromptIdsFormV1::MerkleV1] {
        for prefill_draw in [false, true] {
            let mut f = Fixture::new(4, 2, form, false, artifact);
            f.use_canonical_attempt(prefill_draw);
            assert_ne!(f.class.tokenizer_id, Hash64::default());
            assert_eq!(f.record.tokenizer_id, f.class.tokenizer_id);
            assert_eq!(f.context.tokenizer_id, Hash64::default());
            assert_eq!(f.result.tokenizer, Digest([0; 64]));
            assert_eq!(f.prompt.len(), 31);
            let geometry = f.geometry(limits()).unwrap();
            assert_eq!(geometry.source_units(SourceKind::ArtifactInventory), 4);
            let request = f.request(SourceKind::ArtifactInventory, 0, 1);
            let opening = open_artifact_leaf_v1(&values, 0).unwrap();
            let encoded = borsh::to_vec(&opening).unwrap();
            assert_eq!(encoded.len(), 166);
            assert_eq!(geometry.decode_g14_response_v2(&request, &encoded).unwrap(), (0, StoredUnitV2::Artifact(opening)));
            assert_eq!(geometry.source_units(SourceKind::Input), 1);
            let request = f.request(SourceKind::Input, 0, 1);
            let unit = match form {
                PalwPromptIdsFormV1::Flat => StoredUnitV2::FlatInput(f.prompt.clone()),
                PalwPromptIdsFormV1::MerkleV1 => StoredUnitV2::Input(prompt_ids_opening_v1(&f.prompt, 0).unwrap()),
            };
            let encoded = match &unit {
                StoredUnitV2::FlatInput(ids) => borsh::to_vec(ids).unwrap(),
                StoredUnitV2::Input(opening) => borsh::to_vec(opening).unwrap(),
                _ => unreachable!(),
            };
            assert_eq!(geometry.decode_g14_response_v2(&request, &encoded).unwrap(), (0, unit));
        }
    }
}

#[test]
fn canonical_attempt_tokenizer_changes_cannot_substitute_context_result_or_registered_class() {
    for attack in 0..4 {
        let mut f = Fixture::new(4, 2, PalwPromptIdsFormV1::Flat, false, h(85));
        f.use_canonical_attempt(true);
        match attack {
            0 => f.context.tokenizer_id = f.class.tokenizer_id,
            1 => f.result.tokenizer = Digest(f.class.tokenizer_id.as_bytes()),
            2 => f.record.tokenizer_id = Hash64::default(),
            _ => f.class.tokenizer_id = Hash64::default(),
        }
        assert!(f.geometry(limits()).is_err(), "tokenizer substitution {attack}");
    }
}
#[test]
fn row_pieces_and_tensor_names_are_counted_in_existing_g14_borsh_width() {
    let f = Fixture::new(40_000, 2, PalwPromptIdsFormV1::Flat, false, h(20));
    let geometry = f.geometry(limits()).unwrap();
    assert_eq!(geometry.source_units(SourceKind::ArtifactInventory), 80_000);
    // Each 40,000-byte row is split into 32,768 and 7,232, depth 17 at index0/1.
    assert_eq!(geometry.unit_bound(SourceKind::ArtifactInventory, 0).unwrap().max_unit_bytes, 32_768 + 34 + 17 * 64);
    assert_eq!(geometry.unit_bound(SourceKind::ArtifactInventory, 1).unwrap().max_unit_bytes, 7_232 + 34 + 17 * 64);
    let mut cap = limits();
    cap.max_inventory_leaves = 79_999;
    assert!(matches!(f.geometry(cap), Err(PositiveSourceGeometryErrorV2::Budget)));
}
#[test]
fn flat_input_is_one_full_vec_and_held_override_uses_existing_prompt_form_rule() {
    let flat = Fixture::new(4, 65, PalwPromptIdsFormV1::Flat, false, h(21));
    let geometry = flat.geometry(limits()).unwrap();
    assert_eq!(geometry.source_units(SourceKind::Input), 1);
    let bound = geometry.check_unit(SourceKind::Input, 0, &StoredUnitV2::FlatInput(flat.prompt.clone())).unwrap();
    assert_eq!(bound.max_data_bytes, 260);
    assert_eq!(bound.max_unit_bytes, 264);
    assert!(geometry.check_unit(SourceKind::Input, 1, &StoredUnitV2::FlatInput(flat.prompt)).is_err());
    let held = Fixture::new(4, 65, PalwPromptIdsFormV1::Flat, true, h(21));
    let geometry = held.geometry(limits()).unwrap();
    assert_eq!(geometry.input_form(), PalwPromptIdsFormV1::MerkleV1);
    assert_eq!(geometry.source_units(SourceKind::Input), 3);
    assert!(geometry.check_unit(SourceKind::Input, 0, &StoredUnitV2::FlatInput(held.prompt)).is_err());
}
#[test]
fn forged_registered_job_and_source_metadata_are_refused() {
    for attack in 0..10 {
        let mut f = Fixture::new(4, 2, PalwPromptIdsFormV1::Flat, false, h(22));
        match attack {
            0 => f.record.layout_digest = h(90),
            1 => f.record.tokenizer_id = h(90),
            2 => f.result.artifact = d(90),
            3 => f.result.registered_class = d(90),
            4 => f.result.input = d(90),
            5 => f.context.execution_seed[0] ^= 1,
            6 => f.result.decode = u32::MAX,
            7 => f.record.program_bytes += 1,
            8 => f.class.layout.commit_tiles.push(4),
            _ => f.context.network_id.resize(PALW_V2_MAX_NETWORK_ID_BYTES + 1, 0),
        }
        assert!(f.geometry(limits()).is_err(), "attack {attack}");
    }
    let f = Fixture::new(4, 2, PalwPromptIdsFormV1::Flat, false, h(22));
    let geometry = f.geometry(limits()).unwrap();
    let mut request = f.request(SourceKind::Input, 0, 1);
    request.source = d(90);
    assert_eq!(geometry.request_bound(&request), Err(PositiveSourceGeometryErrorV2::Binding));
    let mut request = f.request(SourceKind::ArtifactInventory, 0, 4);
    request.end = u64::MAX;
    assert!(geometry.request_bound(&request).is_err());
}
#[test]
fn bounded_index_preflight_checks_instance_multiplication_before_expansion() {
    let f = Fixture::new(4, 2, PalwPromptIdsFormV1::Flat, false, h(22));
    let mut p = f.class.decode_program().unwrap();
    // Exercise the allocation-free upper-bound routine directly: the raw
    // candidate is not claimed to be valid registered/native program state.
    p.params[0].per_layer = true;
    p.schedule.layers = vec![0; 1024];
    let mut cap = limits();
    cap.max_parameter_instances = 1023;
    assert_eq!(preflight_inventory_v2(&p, cap), Err(PositiveSourceGeometryErrorV2::Budget));
    p.params[0].shape = vec![u32::MAX; 3];
    let outcome = std::panic::catch_unwind(|| preflight_inventory_v2(&p, limits()));
    assert!(outcome.is_ok());
    assert_eq!(outcome.unwrap(), Err(PositiveSourceGeometryErrorV2::Budget));
    let mut cap = limits();
    cap.max_program_bytes = 1;
    assert!(matches!(f.geometry(cap), Err(PositiveSourceGeometryErrorV2::Budget)));
}

#[test]
fn empty_artifact_inventory_never_creates_a_source_request() {
    let mut p = ProgramBuilder::new(4, 1 << 18);
    let values = p.konst(DType::I8, &[4], &[1, 2, 3, 4]);
    let mut pre = p.block("pre", vec![]);
    let wide = pre.cast(values, DType::I16);
    pre.commit(wide);
    let x = pre.clamp(wide, -128, 127, DType::I8);
    pre.commit(x);
    let pre = pre.finish(&[x]);
    let mut post = p.block("post", vec![TensorType::fixed(DType::I8, &[4])]);
    let y = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(y);
    let Ref::Node(logits) = y else { unreachable!() };
    let post = post.finish(&[]);
    let mut program = p.finish(pre, vec![], post, logits);
    program.logits_scheme_id = tiled_logits_scheme_id_v1().as_bytes();
    let f = Fixture::from_program(program, 2, PalwPromptIdsFormV1::Flat, h(23));
    let mut no_artifact = limits();
    no_artifact.max_parameter_instances = 0;
    no_artifact.max_inventory_leaves = 0;
    let geometry = f.geometry(no_artifact).unwrap();
    assert_eq!(geometry.source_units(SourceKind::ArtifactInventory), 0);
    assert_eq!(
        geometry.validate_request(&f.request(SourceKind::ArtifactInventory, 0, 1)),
        Err(PositiveSourceGeometryErrorV2::Binding)
    );
    assert!(geometry.unit_bound(SourceKind::ArtifactInventory, 0).is_err());
    assert_eq!(geometry.source_units(SourceKind::Input), 1);
}

#[path = "independent_source_ledger.rs"]
mod independent_source_ledger;
