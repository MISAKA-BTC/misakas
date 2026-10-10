//! Exact finite-domain reference for descriptor-scoped model statements. This is not
//! an activation certificate: the consumer must authenticate its scope and charge the
//! derived budget; runtime-pack independence, fidelity and G14 remain separate gates.
use crate::Hash64;
use crate::palw_artifact::{PalwArtifactOperandV1, artifact_leaf_v1, artifact_root_v1};
use crate::palw_tir_artifact_v1::{PalwTirModelInventoryV2, palw_tir_tensor_bytes_v1};
use misaka_palw_kernel::descriptor::{KernelDescriptorV1, k2_tir_v5_descriptor};
use misaka_palw_kernel::hash::{Digest, finish, keyed};
use misaka_palw_kernel::plan::VerificationPlanV1;
use misaka_palw_kernel::trace::{ParamCommitmentsV1, TraceV1};
use misaka_palw_tir::{MapParams, Tensor};
use std::collections::BTreeMap;

pub const MODEL_CONFORMANCE_MAX_CASES_V2: u64 = 1024;
pub const MODEL_CONFORMANCE_MAX_RAW_BYTES_V2: u64 = 64 << 10;
pub const MODEL_CONFORMANCE_MAX_RAM_ENVELOPE_V2: u64 = 16 << 20;
pub const MODEL_CONFORMANCE_MAX_WORK_V2: u64 = 1 << 26;
const DOMAIN: &[u8] = b"misaka-palw/model-conformance/finite-traces/v2";

/// Derived from the complete program and plan, never accepted from a filing.
/// Instances and all public cases are checked before source bytes are decoded.
#[derive(Debug)]
pub struct ModelConformanceDomainV2<'p> {
    inventory: PalwTirModelInventoryV2<'p>,
    max_length: u32,
    encoder: bool,
    cases: u64,
    raw_bytes: u64,
    ram_envelope: u64,
    work: u64,
    plan_root: Digest,
}

fn capped(value: u64, cap: u64, name: &str) -> Result<u64, String> {
    if value > cap { Err(format!("model conformance {name} {value} exceeds {cap}")) } else { Ok(value) }
}

impl<'p> ModelConformanceDomainV2<'p> {
    pub fn new(
        descriptor: &KernelDescriptorV1,
        program: &'p misaka_palw_tir::TirProgramV1,
        plan: &VerificationPlanV1,
    ) -> Result<Self, String> {
        let inventory = PalwTirModelInventoryV2::new(descriptor, program)?;
        let encoder = descriptor.digest() == k2_tir_v5_descriptor().digest();
        let max_length = if encoder {
            if plan.max_positions != 1 {
                return Err("encoder conformance has one kernel position".into());
            }
            misaka_palw_kernel::seg_encoder::encoder_binding_v1(program)?.l
        } else {
            if plan.max_positions == 0 || plan.max_positions > program.history_bound {
                return Err("decoder conformance position bound is outside the program".into());
            }
            plan.max_positions
        };
        // Every nonempty word of length 1..=L. Stateless programs use the same domain:
        // no dependency analysis can silently narrow the public input contract.
        let mut cases = 0u64;
        let mut words = 1u64;
        for _ in 0..max_length {
            words = words.checked_mul(program.token_bound as u64).ok_or("model conformance input count overflows")?;
            cases = capped(cases.saturating_add(words), MODEL_CONFORMANCE_MAX_CASES_V2, "input cases")?;
        }
        if cases == 0 {
            return Err("empty model conformance input domain".into());
        }
        capped(inventory.leaf_count() as u64, MODEL_CONFORMANCE_MAX_CASES_V2, "inventory leaves")?;
        let instances = inventory.instances();
        let mut raw_bytes = 0u64;
        let mut param_cells = 0u64;
        for (j, layers) in instances.iter().enumerate() {
            let bytes = palw_tir_tensor_bytes_v1(program, j as u16).saturating_mul(layers.len() as u64);
            raw_bytes = capped(raw_bytes.saturating_add(bytes), MODEL_CONFORMANCE_MAX_RAW_BYTES_V2, "raw model bytes")?;
            param_cells = param_cells.saturating_add(bytes / program.params[j].dtype.width() as u64);
        }
        // The reference retains every position's values; dynamic H is evaluated at L.
        // Include declared states/constants and encoder inputs even when not read.
        let positions = if encoder { 1 } else { max_length as u64 };
        let mut cells = 0u64;
        let mut operations = 0u64;
        let mut max_arity = 1u64;
        for (b, _) in program.occurrences() {
            for (n, node) in program.blocks[b as usize].nodes.iter().enumerate() {
                cells = cells.saturating_add(node.out.elements_at(positions));
                max_arity = max_arity.max(node.inputs.len() as u64);
                let c = misaka_palw_tir::admit::node_cost(program, b as usize, n, positions);
                operations = operations.saturating_add(c.macs).saturating_add(c.elementwise).saturating_add(c.transcendentals);
            }
        }
        let const_cells = program.consts.iter().map(|c| c.data.len() as u64 / c.dtype.width() as u64).fold(0u64, u64::saturating_add);
        let state_cells = program
            .states
            .iter()
            .map(|s| {
                let shape = s.shape.iter().fold(1u64, |n, d| n.saturating_mul(*d as u64));
                shape.saturating_mul(positions).saturating_mul(if s.per_layer { program.schedule.layers.len() as u64 } else { 1 })
            })
            .fold(0u64, u64::saturating_add);
        let input_cells = if encoder { max_length as u64 + 1 } else { max_length as u64 };
        let trace_cells = cells.saturating_mul(positions);
        let live_cells = trace_cells
            .saturating_add(param_cells)
            .saturating_add(const_cells)
            .saturating_add(state_cells)
            .saturating_add(input_cells);
        // Conservative reference envelope: retained trace, cloned input operands and
        // primitive scratch (32 copies), plus program/wiring/container metadata. It is
        // not an execution-credit tariff or a complete network resource contract.
        let ram_envelope = live_cells
            .saturating_mul(16)
            .saturating_mul(max_arity.saturating_add(32))
            .saturating_add((program.encode().len() as u64).saturating_mul(64))
            .saturating_add(raw_bytes.saturating_mul(8));
        capped(ram_envelope, MODEL_CONFORMANCE_MAX_RAM_ENVELOPE_V2, "reference RAM envelope")?;
        let work = cases
            .saturating_mul(
                operations
                    .saturating_add(cells.saturating_mul(32))
                    .saturating_add(state_cells)
                    .saturating_add(const_cells)
                    .saturating_add(input_cells)
                    .max(1),
            )
            .saturating_mul(positions)
            .saturating_mul(64)
            .saturating_add(raw_bytes.saturating_mul(64));
        capped(work, MODEL_CONFORMANCE_MAX_WORK_V2, "reference work")?;
        let schedule = misaka_palw_kernel::descriptor::KernelScheduleV1::default()
            .with(descriptor.digest(), misaka_palw_kernel::descriptor::KernelStatusV1::Active { since_daa: 0 });
        let range_rule = if encoder {
            let binding = misaka_palw_kernel::seg_encoder::encoder_binding_v1(program)?;
            misaka_palw_kernel::seg_encoder::prove_encoder_ranges_v1(program, &binding)?;
            misaka_palw_kernel::check::RangeRuleV1::ProvenByV2
        } else {
            misaka_palw_kernel::check::RangeRuleV1::TirV1
        };
        misaka_palw_kernel::check::check_plan_with_v1(
            &schedule,
            descriptor,
            program,
            misaka_palw_kernel::public::program_root_v1(&program.encode()),
            plan,
            0,
            range_rule,
        )
        .map_err(|e| e.to_string())?;
        Ok(Self { inventory, max_length, encoder, cases, raw_bytes, ram_envelope, work, plan_root: plan.root() })
    }
    pub fn cases(&self) -> u64 {
        self.cases
    }
    pub fn raw_bytes(&self) -> u64 {
        self.raw_bytes
    }
    pub fn ram_envelope(&self) -> u64 {
        self.ram_envelope
    }
    pub fn work(&self) -> u64 {
        self.work
    }

    /// Length-major, then lexicographic token order. The callback sees only one
    /// bounded word at a time; empty prompts and noncanonical padding are not cases.
    pub fn visit_inputs(&self, visit: &mut dyn FnMut(&[u32]) -> Result<(), String>) -> Result<(), String> {
        let radix = self.inventory.program().token_bound;
        for len in 1..=self.max_length {
            let mut word = vec![0u32; len as usize];
            loop {
                visit(&word)?;
                let mut i = word.len();
                loop {
                    if i == 0 {
                        break;
                    }
                    i -= 1;
                    word[i] += 1;
                    if word[i] < radix {
                        break;
                    }
                    word[i] = 0;
                }
                if i == 0 && word[0] == 0 {
                    break;
                }
            }
        }
        Ok(())
    }

    /// Compare all claimed implementation roots with the reference over this full
    /// scope. These are equality checks, not evidence of who ran an implementation.
    pub fn verify(
        &self,
        operands: &[PalwArtifactOperandV1],
        model_root: Hash64,
        pc_root: Digest,
        implementation_roots: [Digest; 3],
    ) -> Result<ModelConformanceReferenceV2, String> {
        let reference = self.reference(operands)?;
        if reference.model_root != model_root || reference.pc_root != pc_root {
            return Err("model conformance source/parameter relation differs".into());
        }
        if implementation_roots.iter().any(|r| *r != reference.trace_root) {
            return Err("model conformance implementation trace differs".into());
        }
        Ok(reference)
    }

    /// Recompute both inventory and v3 PC roots from the same authenticated bytes,
    /// then every node value of every allowed word, with state initialized by the
    /// reference on each word. Job ids/count are injected; they never enter PC/R.
    pub fn reference(&self, operands: &[PalwArtifactOperandV1]) -> Result<ModelConformanceReferenceV2, String> {
        if operands.len() != self.inventory.leaf_count() as usize {
            return Err("model conformance inventory count differs".into());
        }
        let actual_bytes =
            operands.iter().try_fold(0u64, |n, o| n.checked_add(o.bytes.len() as u64)).ok_or("operand byte count overflows")?;
        if actual_bytes != self.raw_bytes {
            return Err("model conformance inventory bytes differ".into());
        }
        let mut rows = Vec::with_capacity(operands.len());
        self.inventory.visit_rows(&mut |r| rows.push(r)).map_err(|e| e.to_string())?;
        let p = self.inventory.program();
        // Validate all coordinates before any tensor allocation.
        for (o, r) in operands.iter().zip(&rows) {
            if o.tensor_name != p.params[r.param as usize].name
                || o.layer != r.layer
                || o.row_start != r.row_start
                || o.bytes.len() != r.len as usize
            {
                return Err("model conformance inventory coordinate differs".into());
            }
        }
        let model_root =
            artifact_root_v1(&operands.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).ok_or("empty model inventory")?;
        let mut bytes_of: BTreeMap<(u16, Option<u16>), Vec<u8>> = BTreeMap::new();
        for (o, r) in operands.iter().zip(&rows) {
            bytes_of.entry((r.param, r.layer)).or_default().extend_from_slice(&o.bytes);
        }
        let mut params = MapParams::default();
        for ((j, layer), data) in bytes_of {
            let d = &p.params[j as usize];
            let shape = d.shape.iter().map(|d| *d as usize).collect::<Vec<_>>();
            params.tensors.insert((j, layer), Tensor::from_le_bytes(d.dtype, &shape, &data).map_err(|e| e.to_string())?);
        }
        let pc_root = ParamCommitmentsV1::of_v3(&params).root();
        let mut hash = keyed(DOMAIN);
        hash.update(&self.inventory.descriptor_digest());
        hash.update(&misaka_palw_kernel::public::program_root_v1(&p.encode()));
        hash.update(&self.plan_root);
        hash.update(model_root.as_byte_slice());
        hash.update(&pc_root);
        hash.update(&self.cases.to_le_bytes());
        let binding = if self.encoder { Some(misaka_palw_kernel::seg_encoder::encoder_binding_v1(p)?) } else { None };
        self.visit_inputs(&mut |word| {
            let trace = match &binding {
                Some(b) => misaka_palw_kernel::seg_encoder::trace_encoder_v1(p, &params, b, word)?,
                None => misaka_palw_kernel::trace::trace_v1(p, &params, word).map_err(|e| e.to_string())?,
            };
            hash.update(&(word.len() as u64).to_le_bytes());
            for t in word {
                hash.update(&t.to_le_bytes());
            }
            hash_trace(&mut hash, &trace);
            Ok(())
        })?;
        Ok(ModelConformanceReferenceV2 { model_root, pc_root, trace_root: finish(hash), cases: self.cases })
    }
}

fn hash_tensor(hash: &mut blake2b_simd::State, t: &Tensor) {
    hash.update(&[t.dtype.tag()]);
    hash.update(&(t.shape.len() as u64).to_le_bytes());
    for d in &t.shape {
        hash.update(&(*d as u64).to_le_bytes());
    }
    hash.update(&(t.data.len() as u64).to_le_bytes());
    for v in &t.data {
        hash.update(&v.to_le_bytes());
    }
}
fn hash_trace(hash: &mut blake2b_simd::State, trace: &TraceV1) {
    hash.update(&(trace.values.len() as u64).to_le_bytes());
    for pos in &trace.values {
        hash.update(&(pos.len() as u64).to_le_bytes());
        for occ in pos {
            hash.update(&(occ.len() as u64).to_le_bytes());
            for t in occ {
                hash_tensor(hash, t);
            }
        }
    }
    hash.update(&(trace.inputs.len() as u64).to_le_bytes());
    for pos in &trace.inputs {
        hash.update(&(pos.len() as u64).to_le_bytes());
        for t in pos {
            hash_tensor(hash, t);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelConformanceReferenceV2 {
    pub model_root: Hash64,
    pub pc_root: Digest,
    pub trace_root: Digest,
    pub cases: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_kernel::descriptor::{k2_tir_v4_descriptor, k2_tir_v5_descriptor};
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, Ref};
    use misaka_palw_tir::{DType, TensorType, TirProgramV1};

    fn program(encoder: bool, stateful: bool, tokens: u32, length: u32) -> TirProgramV1 {
        let mut b = ProgramBuilder::new(tokens, HISTORY_BOUND_V1_SMALL);
        let table = b.param("weights", DType::I8, &[tokens, 2], false);
        let (ids, count) = if encoder {
            (b.param("input.ids", DType::Idx, &[length], false), Some(b.param("input.count", DType::Idx, &[], false)))
        } else {
            (Ref::Input(0), None)
        };
        let state = if stateful { Some(b.fixed_state("memory", DType::I8, &[2], -100, 100, false)) } else { None };
        let shape = if encoder { vec![length * 2] } else { vec![2] };
        let pre = {
            let mut block = b.block("pre", vec![]);
            let x = block.gather(table, ids, 0, 0);
            let mut x = block.cast(x, DType::I32);
            if encoder {
                x = block.reshape_fixed(x, &shape);
            }
            if let Some(c) = count {
                let c = block.cast(c, DType::I32);
                x = block.add(x, c, DType::I32);
            }
            if let Some(s) = state {
                let next = block.add(x, Ref::State(s), DType::I32);
                let next = block.clamp(next, -100, 100, DType::I8);
                let next = block.state_write(s, next);
                x = block.cast(next, DType::I32);
            }
            block.finish(&[x])
        };
        let post = {
            let mut block = b.block("post", vec![TensorType::fixed(DType::I32, &shape)]);
            let logits = block.clamp(Ref::CarryIn(0), i32::MIN as i64, i32::MAX as i64, DType::I32);
            block.commit(logits);
            block.finish(&[])
        };
        b.finish(pre, vec![], post, 0)
    }
    fn plan(d: &KernelDescriptorV1, p: &TirProgramV1, positions: u32) -> VerificationPlanV1 {
        misaka_palw_kernel::plan::plan_for_tir_program_v1(d, p, misaka_palw_kernel::public::program_root_v1(&p.encode()), positions)
            .unwrap()
    }
    fn operands(domain: &ModelConformanceDomainV2<'_>, value: u8) -> Vec<PalwArtifactOperandV1> {
        let mut out = Vec::new();
        let p = domain.inventory.program();
        domain
            .inventory
            .visit_rows(&mut |r| {
                out.push(PalwArtifactOperandV1 {
                    tensor_name: p.params[r.param as usize].name.clone(),
                    layer: r.layer,
                    row_start: r.row_start,
                    bytes: vec![value; r.len as usize],
                })
            })
            .unwrap();
        out
    }

    #[test]
    fn complete_model_domain_enumerates_all_lengths_and_mixed_inputs_for_encoder_and_stateful_decoder() {
        for encoder in [false, true] {
            let d = if encoder { k2_tir_v5_descriptor() } else { k2_tir_v4_descriptor() };
            let p = program(encoder, !encoder, 2, 3);
            let plan = plan(&d, &p, if encoder { 1 } else { 3 });
            let domain = ModelConformanceDomainV2::new(&d, &p, &plan).unwrap();
            let mut words = Vec::new();
            domain
                .visit_inputs(&mut |w| {
                    words.push(w.to_vec());
                    Ok(())
                })
                .unwrap();
            assert_eq!(
                words,
                vec![
                    vec![0],
                    vec![1],
                    vec![0, 0],
                    vec![0, 1],
                    vec![1, 0],
                    vec![1, 1],
                    vec![0, 0, 0],
                    vec![0, 0, 1],
                    vec![0, 1, 0],
                    vec![0, 1, 1],
                    vec![1, 0, 0],
                    vec![1, 0, 1],
                    vec![1, 1, 0],
                    vec![1, 1, 1]
                ]
            );
            assert_eq!(domain.cases(), 14);
            let raw = operands(&domain, 3);
            let a = domain.reference(&raw).unwrap();
            assert_eq!(a, domain.reference(&raw).unwrap(), "each word starts from reference-defined initial state");
            let changed = domain.reference(&operands(&domain, 4)).unwrap();
            assert_ne!(a.model_root, changed.model_root);
            assert_ne!(a.pc_root, changed.pc_root);
            assert_ne!(a.trace_root, changed.trace_root);
            assert_eq!(a.cases, 14);
            assert_eq!(domain.verify(&raw, a.model_root, a.pc_root, [a.trace_root; 3]).unwrap(), a);
            assert!(domain.verify(&raw, a.model_root, changed.pc_root, [a.trace_root; 3]).is_err());
            assert!(domain.verify(&raw, changed.model_root, a.pc_root, [a.trace_root; 3]).is_err());
            assert!(domain.verify(&raw, a.model_root, a.pc_root, [a.trace_root, a.trace_root, [9; 64]]).is_err());
            let params = MapParams { tensors: [((0, None), Tensor::new(DType::I8, vec![2, 2], vec![3; 4]).unwrap())].into() };
            assert_eq!(a.pc_root, ParamCommitmentsV1::of_v3(&params).root(), "encoder job parameters do not enter model PC");
        }
    }

    #[test]
    fn complete_model_domain_refuses_exponential_inputs_unknown_semantics_and_unbound_plans() {
        let d = k2_tir_v5_descriptor();
        let p = program(true, false, 256, 12);
        assert!(ModelConformanceDomainV2::new(&d, &p, &plan(&d, &p, 1)).unwrap_err().contains("input cases"));
        let p = program(true, false, 1, 4);
        let valid = plan(&d, &p, 1);
        let one = ModelConformanceDomainV2::new(&d, &p, &valid).unwrap();
        let mut words = Vec::new();
        one.visit_inputs(&mut |w| {
            words.push(w.to_vec());
            Ok(())
        })
        .unwrap();
        assert_eq!(words, vec![vec![0], vec![0; 2], vec![0; 3], vec![0; 4]]);
        let mut wrong = valid.clone();
        wrong.max_positions = 2;
        assert!(ModelConformanceDomainV2::new(&d, &p, &wrong).is_err());
        let mut wrong = valid;
        wrong.program_root = [7; 64];
        assert!(ModelConformanceDomainV2::new(&d, &p, &wrong).is_err());
        let mut unknown = d;
        unknown.memory_model_id += 1;
        assert!(ModelConformanceDomainV2::new(&unknown, &p, &plan(&k2_tir_v5_descriptor(), &p, 1)).is_err());
    }

    #[test]
    fn complete_model_inventory_refuses_missing_surplus_and_reordered_or_relabelled_bytes() {
        let d = k2_tir_v5_descriptor();
        let p = program(true, false, 2, 2);
        let domain = ModelConformanceDomainV2::new(&d, &p, &plan(&d, &p, 1)).unwrap();
        let raw = operands(&domain, 2);
        assert!(domain.reference(&raw).is_ok());
        assert!(domain.reference(&[]).is_err());
        let mut surplus = raw.clone();
        surplus.push(raw[0].clone());
        assert!(domain.reference(&surplus).is_err());
        let mut shifted = raw.clone();
        shifted[0].row_start = 1;
        assert!(domain.reference(&shifted).is_err());
        let mut fake = raw.clone();
        fake[0].tensor_name = "input.ids".into();
        assert!(domain.reference(&fake).is_err());
        let mut short = raw;
        short[0].bytes.pop();
        assert!(domain.reference(&short).is_err());
    }
    #[test]
    fn complete_model_reference_preserves_prefix_state_and_hashes_non_output_values() {
        let p = program(false, true, 2, 3);
        let params = MapParams { tensors: [((0, None), Tensor::new(DType::I8, vec![2, 2], vec![1, 2, 3, 4]).unwrap())].into() };
        let trace = misaka_palw_kernel::trace::trace_v1(&p, &params, &[0, 1, 1]).unwrap();
        let post = p.occurrences().len() - 1;
        assert_eq!(
            trace.values.iter().map(|v| v[post][p.logits as usize].data.clone()).collect::<Vec<_>>(),
            vec![vec![1, 2], vec![4, 6], vec![7, 10]]
        );
        let reset = misaka_palw_kernel::trace::trace_v1(&p, &params, &[1]).unwrap();
        assert_eq!(reset.values[0][post][p.logits as usize].data, vec![3, 4]);
        let mut a = keyed(DOMAIN);
        hash_trace(&mut a, &trace);
        let mut forged = trace.clone();
        forged.values[0][0][0].data[0] += 1;
        assert_eq!(
            forged.values[0][post][p.logits as usize], trace.values[0][post][p.logits as usize],
            "the result itself is unchanged"
        );
        let mut b = keyed(DOMAIN);
        hash_trace(&mut b, &forged);
        assert_ne!(finish(a), finish(b), "a hidden intermediate lie cannot borrow the output digest");
    }

    #[test]
    fn complete_model_domain_prices_large_source_tensors_before_decoding() {
        let d = k2_tir_v4_descriptor();
        for (size, reason) in [(70000, "raw model bytes"), (50000, "reference RAM envelope")] {
            let mut p = program(false, false, 2, 1);
            // A genuinely used wide embedding, not a dead declaration rejected by normal form.
            let width = size / 2;
            p.params[0].shape[1] = width;
            for block in &mut p.blocks {
                for node in &mut block.nodes {
                    node.out = TensorType::fixed(node.out.dtype, &[width]);
                }
                for carry in &mut block.carry_in {
                    *carry = TensorType::fixed(carry.dtype, &[width]);
                }
            }
            let error = ModelConformanceDomainV2::new(&d, &p, &plan(&d, &p, 1)).unwrap_err();
            assert!(error.contains(reason), "{error}");
        }
    }

    #[test]
    fn complete_model_domain_keeps_real_encoder_and_decoder_vocabularies_outside_enumeration() {
        use borsh::BorshDeserialize;
        use std::io::Cursor;
        let mut witness = Cursor::new(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../docs/design/palw/tir/evidence/rfc02-bert-model-artifact-court.borsh"
            ))
            .as_slice(),
        );
        assert_eq!(u16::deserialize_reader(&mut witness).unwrap(), 2);
        let d = KernelDescriptorV1::deserialize_reader(&mut witness).unwrap();
        let bytes = Vec::<u8>::deserialize_reader(&mut witness).unwrap();
        let encoder = TirProgramV1::decode_canonical(&bytes).unwrap();
        assert!(ModelConformanceDomainV2::new(&d, &encoder, &plan(&d, &encoder, 1)).unwrap_err().contains("input cases"));
        let decoder = TirProgramV1::decode_canonical(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/design/palw/tir/evidence/qwen25-real-declared-program.tir"
        )))
        .unwrap();
        let d = k2_tir_v4_descriptor();
        assert!(ModelConformanceDomainV2::new(&d, &decoder, &plan(&d, &decoder, 32)).unwrap_err().contains("input cases"));
    }
}
