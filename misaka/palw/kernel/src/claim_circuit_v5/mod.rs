//! Exact semantic and native hash circuits for a future proof contract.
//! This module does not register a V4/V5 claim, provide a PCS/GKR proof, or grant
//! a review. Every emitted gate has executable Boolean semantics; cost is the
//! actual compiled graph and bounded witness, not an operation-name inventory.
use crate::hash::{Digest, object_id};
use borsh::{BorshDeserialize, BorshSerialize};
pub mod bits;
pub mod blake2b;
pub mod native_commitment;
pub mod scalar;
pub mod tensor;
pub type WireV5 = u32;
pub type ByteV5 = [WireV5; 8];
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BooleanGateV5 {
    Constant(bool),
    Input { ordinal: u32 },
    Xor { a: WireV5, b: WireV5 },
    And { a: WireV5, b: WireV5 },
    Not { a: WireV5 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CircuitLimitsV5 {
    pub maximum_gates: u32,
    pub maximum_input_bits: u32,
    pub maximum_assertions: u32,
    pub maximum_graph_bytes: u64,
    pub maximum_witness_bytes: u64,
}
#[derive(Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BooleanCircuitV5 {
    version: u16,
    input_bits: u32,
    gates: Vec<BooleanGateV5>,
    assertions: Vec<(WireV5, bool)>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CircuitCostV5 {
    pub gates: u64,
    pub input_bits: u64,
    pub assertions: u64,
    pub graph_bytes: u64,
    pub witness_bytes: u64,
    pub xor_gates: u64,
    pub and_gates: u64,
    pub not_gates: u64,
}
impl BooleanCircuitV5 {
    pub fn input_bits(&self) -> u32 {
        self.input_bits
    }
    pub fn gates(&self) -> &[BooleanGateV5] {
        &self.gates
    }
    pub fn assertions(&self) -> &[(WireV5, bool)] {
        &self.assertions
    }
    pub fn id(&self) -> Digest {
        object_id(b"MISAKA/PALW/BOOLEAN-CIRCUIT/V5", self)
    }
    pub fn cost(&self) -> Result<CircuitCostV5, String> {
        let gates = u64::try_from(self.gates.len()).map_err(|_| "circuit gate length overflow")?;
        let assertions = u64::try_from(self.assertions.len()).map_err(|_| "circuit assertion length overflow")?;
        let graph_bytes = gates
            .checked_mul(std::mem::size_of::<BooleanGateV5>() as u64)
            .and_then(|n| n.checked_add(assertions.checked_mul(std::mem::size_of::<(WireV5, bool)>() as u64)?))
            .ok_or("circuit graph bytes overflow")?;
        Ok(CircuitCostV5 {
            gates,
            input_bits: u64::from(self.input_bits),
            assertions,
            graph_bytes,
            witness_bytes: gates,
            xor_gates: self.gates.iter().filter(|g| matches!(g, BooleanGateV5::Xor { .. })).count() as u64,
            and_gates: self.gates.iter().filter(|g| matches!(g, BooleanGateV5::And { .. })).count() as u64,
            not_gates: self.gates.iter().filter(|g| matches!(g, BooleanGateV5::Not { .. })).count() as u64,
        })
    }
    pub fn validate(&self, limits: CircuitLimitsV5) -> Result<CircuitCostV5, String> {
        let cost = self.cost()?;
        if self.version != 5
            || cost.gates > u64::from(limits.maximum_gates)
            || self.input_bits > limits.maximum_input_bits
            || cost.assertions > u64::from(limits.maximum_assertions)
            || cost.graph_bytes > limits.maximum_graph_bytes
            || cost.witness_bytes > limits.maximum_witness_bytes
        {
            return Err("circuit graph/input/assertion/witness capacity exceeded".into());
        }
        let mut ordinal = 0u32;
        for (index, gate) in self.gates.iter().enumerate() {
            let before = |w: WireV5| u64::from(w) < index as u64;
            match gate {
                BooleanGateV5::Input { ordinal: i } => {
                    if *i != ordinal {
                        return Err("noncanonical circuit input order".into());
                    }
                    ordinal = ordinal.checked_add(1).ok_or("circuit input ordinal overflow")?;
                }
                BooleanGateV5::Constant(_) => {}
                BooleanGateV5::Xor { a, b } | BooleanGateV5::And { a, b } if before(*a) && before(*b) => {}
                BooleanGateV5::Not { a } if before(*a) => {}
                _ => return Err("circuit reference is forward, cyclic or out of range".into()),
            }
        }
        if ordinal != self.input_bits || self.assertions.iter().any(|(w, _)| *w as usize >= self.gates.len()) {
            return Err("circuit input count/output reference mismatch".into());
        }
        Ok(cost)
    }
    /// Exact local witness evaluator. This is a reference relation, not a
    /// succinct proof or authority to label a constraint system approved.
    pub(crate) fn evaluate(&self, inputs: &[bool], limits: CircuitLimitsV5) -> Result<Vec<bool>, String> {
        self.validate(limits)?;
        if inputs.len() != self.input_bits as usize {
            return Err("circuit input length mismatch".into());
        }
        let mut values = Vec::new();
        values.try_reserve_exact(self.gates.len()).map_err(|e| e.to_string())?;
        for gate in &self.gates {
            let value = match *gate {
                BooleanGateV5::Constant(v) => v,
                BooleanGateV5::Input { ordinal } => inputs[ordinal as usize],
                BooleanGateV5::Xor { a, b } => values[a as usize] ^ values[b as usize],
                BooleanGateV5::And { a, b } => values[a as usize] & values[b as usize],
                BooleanGateV5::Not { a } => !values[a as usize],
            };
            values.push(value);
        }
        if self.assertions.iter().any(|(w, expected)| values[*w as usize] != *expected) {
            return Err("circuit public assertion failed".into());
        }
        Ok(values)
    }
}
pub struct CircuitBuilderV5 {
    pub(crate) circuit: BooleanCircuitV5,
    limits: CircuitLimitsV5,
    planned: Option<(CircuitCostV5, CircuitMemoryBudgetV5, CircuitMemoryLeaseV5)>,
    counting: Option<CircuitCostV5>,
}
impl CircuitBuilderV5 {
    pub fn constant(&self, value: bool) -> WireV5 {
        u32::from(value)
    }
    fn valid(&self, wire: WireV5) -> Result<(), String> {
        if u64::from(wire) >= self.gate_count() { Err("unknown circuit wire".into()) } else { Ok(()) }
    }
    fn push(&mut self, gate: BooleanGateV5) -> Result<WireV5, String> {
        if let Some(cost) = self.counting.as_mut() {
            let id = u32::try_from(cost.gates).map_err(|_| "circuit counting wire overflow")?;
            cost.gates = cost.gates.checked_add(1).ok_or("circuit counting gates overflow")?;
            cost.witness_bytes = cost.gates;
            cost.graph_bytes =
                cost.graph_bytes.checked_add(std::mem::size_of::<BooleanGateV5>() as u64).ok_or("circuit counting graph overflow")?;
            match gate {
                BooleanGateV5::Input { .. } => cost.input_bits += 1,
                BooleanGateV5::Xor { .. } => cost.xor_gates += 1,
                BooleanGateV5::And { .. } => cost.and_gates += 1,
                BooleanGateV5::Not { .. } => cost.not_gates += 1,
                _ => {}
            }
            if cost.gates > u64::from(self.limits.maximum_gates)
                || cost.graph_bytes > self.limits.maximum_graph_bytes
                || cost.witness_bytes > self.limits.maximum_witness_bytes
            {
                return Err("semantic counting exceeds gate/allocation capacity".into());
            }
            return Ok(id);
        }
        let n = self.circuit.gates.len().checked_add(1).ok_or("circuit gate count overflow")?;
        if n > self.limits.maximum_gates as usize || n as u64 > self.limits.maximum_witness_bytes {
            return Err("circuit gate/witness capacity exhausted".into());
        }
        if self.planned.as_ref().is_some_and(|(p, _, _)| n as u64 > p.gates) {
            return Err("emitted gate count exceeds semantic preflight".into());
        }
        if n > self.circuit.gates.capacity() {
            return Err("circuit graph exceeds its exact reserved allocation".into());
        }
        let id = u32::try_from(self.circuit.gates.len()).map_err(|_| "circuit wire overflow")?;
        self.circuit.gates.push(gate);
        Ok(id)
    }
    fn input(&mut self) -> Result<WireV5, String> {
        let ordinal = self.circuit.input_bits;
        if ordinal >= self.limits.maximum_input_bits {
            return Err("circuit input capacity exhausted".into());
        }
        let wire = self.push(BooleanGateV5::Input { ordinal })?;
        self.circuit.input_bits = ordinal + 1;
        Ok(wire)
    }
    pub fn xor(&mut self, a: WireV5, b: WireV5) -> Result<WireV5, String> {
        self.valid(a)?;
        self.valid(b)?;
        self.push(BooleanGateV5::Xor { a, b })
    }
    pub fn and(&mut self, a: WireV5, b: WireV5) -> Result<WireV5, String> {
        self.valid(a)?;
        self.valid(b)?;
        self.push(BooleanGateV5::And { a, b })
    }
    pub fn not(&mut self, a: WireV5) -> Result<WireV5, String> {
        self.valid(a)?;
        self.push(BooleanGateV5::Not { a })
    }
    pub fn assert(&mut self, wire: WireV5, value: bool) -> Result<(), String> {
        self.valid(wire)?;
        if let Some(cost) = self.counting.as_mut() {
            cost.assertions = cost.assertions.checked_add(1).ok_or("circuit counting assertion overflow")?;
            cost.graph_bytes = cost
                .graph_bytes
                .checked_add(std::mem::size_of::<(WireV5, bool)>() as u64)
                .ok_or("circuit counting assertion bytes overflow")?;
            if cost.assertions > u64::from(self.limits.maximum_assertions) || cost.graph_bytes > self.limits.maximum_graph_bytes {
                return Err("semantic counting assertion capacity exceeded".into());
            }
            return Ok(());
        }
        let n = self.circuit.assertions.len().checked_add(1).ok_or("circuit assertion count overflow")?;
        if n > self.limits.maximum_assertions as usize {
            return Err("circuit assertion capacity exhausted".into());
        }
        if self.planned.as_ref().is_some_and(|(p, _, _)| n as u64 > p.assertions) {
            return Err("emitted assertion count exceeds semantic preflight".into());
        }
        if n > self.circuit.assertions.capacity() {
            return Err("circuit assertions exceed exact reserved allocation".into());
        }
        self.circuit.assertions.push((wire, value));
        Ok(())
    }
    pub fn assert_bytes(&mut self, wires: &[ByteV5], expected: &[u8]) -> Result<(), String> {
        if wires.len() != expected.len() {
            return Err("circuit public byte output length mismatch".into());
        }
        for (byte, value) in wires.iter().zip(expected) {
            for (i, wire) in byte.iter().enumerate() {
                self.assert(*wire, (value >> i) & 1 != 0)?;
            }
        }
        Ok(())
    }
}
pub fn input_bits_v5(bytes: &[u8]) -> Vec<bool> {
    bytes.iter().flat_map(|b| (0..8).map(move |i| (b >> i) & 1 != 0)).collect()
}
pub fn output_bytes_v5(wires: &[ByteV5], witness: &[bool]) -> Result<Vec<u8>, String> {
    wires
        .iter()
        .map(|byte| {
            let mut v = 0;
            for (i, wire) in byte.iter().enumerate() {
                let b = witness.get(*wire as usize).ok_or("circuit output outside witness")?;
                v |= u8::from(*b) << i;
            }
            Ok(v)
        })
        .collect()
}

/// Allocation-request budget retained for the entire owner lifetime. This is
/// deterministic typed memory accounting; it does not assert a physical RSS or
/// allocator-overhead approval. A successful reserve cannot be copied or reset.
#[derive(Clone)]
pub struct CircuitMemoryBudgetV5(std::rc::Rc<CircuitMemoryStateV5>);
struct CircuitMemoryStateV5 {
    maximum: u64,
    held: std::cell::Cell<u64>,
}
pub struct CircuitMemoryLeaseV5 {
    state: std::rc::Rc<CircuitMemoryStateV5>,
    bytes: u64,
}
impl Drop for CircuitMemoryLeaseV5 {
    fn drop(&mut self) {
        self.state.held.set(self.state.held.get() - self.bytes);
    }
}
impl CircuitMemoryBudgetV5 {
    pub fn new(maximum: u64) -> Self {
        Self(std::rc::Rc::new(CircuitMemoryStateV5 { maximum, held: std::cell::Cell::new(0) }))
    }
    pub fn held(&self) -> u64 {
        self.0.held.get()
    }
    pub fn reserve(&self, bytes: u64) -> Result<CircuitMemoryLeaseV5, String> {
        let held =
            self.0.held.get().checked_add(bytes).filter(|n| *n <= self.0.maximum).ok_or("circuit memory owner budget exhausted")?;
        self.0.held.set(held);
        Ok(CircuitMemoryLeaseV5 { state: self.0.clone(), bytes })
    }
}
pub struct ReservedCircuitV5 {
    circuit: BooleanCircuitV5,
    memory: CircuitMemoryBudgetV5,
    _lease: CircuitMemoryLeaseV5,
}
pub struct CircuitWitnessV5 {
    values: Vec<bool>,
    _lease: CircuitMemoryLeaseV5,
}
impl CircuitWitnessV5 {
    pub fn values(&self) -> &[bool] {
        &self.values
    }
}
impl ReservedCircuitV5 {
    pub fn circuit(&self) -> &BooleanCircuitV5 {
        &self.circuit
    }
    pub fn evaluate(&self, inputs: &[bool], limits: CircuitLimitsV5) -> Result<CircuitWitnessV5, String> {
        let cost = self.circuit.validate(limits)?;
        let lease = self.memory.reserve(cost.witness_bytes)?;
        let values = self.circuit.evaluate(inputs, limits)?;
        Ok(CircuitWitnessV5 { values, _lease: lease })
    }
}
impl CircuitBuilderV5 {
    /// A compiler must compute the actual semantic graph plan before this call.
    /// Both exact Vec allocations are authorized by a live reservation first.
    pub fn new_planned(limits: CircuitLimitsV5, plan: CircuitCostV5, memory: CircuitMemoryBudgetV5) -> Result<Self, String> {
        if plan.gates > u64::from(limits.maximum_gates)
            || plan.input_bits > u64::from(limits.maximum_input_bits)
            || plan.assertions > u64::from(limits.maximum_assertions)
            || plan.graph_bytes > limits.maximum_graph_bytes
            || plan.witness_bytes > limits.maximum_witness_bytes
            || plan.gates < 2
        {
            return Err("circuit semantic preflight exceeds limits".into());
        }
        let expected = plan
            .gates
            .checked_mul(std::mem::size_of::<BooleanGateV5>() as u64)
            .and_then(|n| n.checked_add(plan.assertions.checked_mul(std::mem::size_of::<(WireV5, bool)>() as u64)?))
            .ok_or("circuit planned graph overflow")?;
        if expected != plan.graph_bytes || plan.witness_bytes != plan.gates {
            return Err("circuit typed allocation plan mismatch".into());
        }
        let lease = memory.reserve(plan.graph_bytes)?;
        let mut circuit = BooleanCircuitV5 { version: 5, input_bits: 0, gates: Vec::new(), assertions: Vec::new() };
        circuit
            .gates
            .try_reserve_exact(usize::try_from(plan.gates).map_err(|_| "circuit gates not addressable")?)
            .map_err(|e| e.to_string())?;
        circuit
            .assertions
            .try_reserve_exact(usize::try_from(plan.assertions).map_err(|_| "circuit assertions not addressable")?)
            .map_err(|e| e.to_string())?;
        if circuit.gates.capacity() as u64 != plan.gates || circuit.assertions.capacity() as u64 != plan.assertions {
            return Err("allocator Vec capacity differs from the exact typed circuit plan".into());
        }
        let mut builder = Self { circuit, limits, planned: Some((plan, memory, lease)), counting: None };
        builder.push(BooleanGateV5::Constant(false))?;
        builder.push(BooleanGateV5::Constant(true))?;
        Ok(builder)
    }
    pub fn finish_reserved(self) -> Result<ReservedCircuitV5, String> {
        let cost = self.circuit.validate(self.limits)?;
        let (planned, memory, lease) = self.planned.ok_or("circuit has no allocation owner")?;
        if cost != planned {
            return Err("emitted semantic graph differs from preflight gate counts".into());
        }
        Ok(ReservedCircuitV5 { circuit: self.circuit, memory, _lease: lease })
    }
}
/// A byte stream references existing circuit wires without a Vec of byte/bit
/// indices. The hash compiler reads one fixed 128-byte block at a time.
pub trait CircuitByteStreamV5 {
    fn len(&self) -> usize;
    fn byte(&self, index: usize) -> Result<ByteV5, String>;
}
impl CircuitByteStreamV5 for [ByteV5] {
    fn len(&self) -> usize {
        <[ByteV5]>::len(self)
    }
    fn byte(&self, index: usize) -> Result<ByteV5, String> {
        self.get(index).copied().ok_or("circuit byte outside stream".into())
    }
}
pub struct ConstantByteStreamV5<'a>(pub &'a [u8]);
impl CircuitByteStreamV5 for ConstantByteStreamV5<'_> {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn byte(&self, index: usize) -> Result<ByteV5, String> {
        let b = self.0.get(index).ok_or("constant stream index")?;
        Ok(std::array::from_fn(|i| u32::from((b >> i) & 1 != 0)))
    }
}
#[derive(Clone, Copy, Debug)]
pub struct InputByteSpanV5 {
    pub first: WireV5,
    pub bytes: usize,
}
impl CircuitByteStreamV5 for InputByteSpanV5 {
    fn len(&self) -> usize {
        self.bytes
    }
    fn byte(&self, index: usize) -> Result<ByteV5, String> {
        if index >= self.bytes {
            return Err("circuit input span index".into());
        }
        let base = u32::try_from(index.checked_mul(8).ok_or("circuit span overflow")?).map_err(|_| "circuit span width overflow")?;
        let base = self.first.checked_add(base).ok_or("circuit span wire overflow")?;
        let mut out = [0; 8];
        for (i, w) in out.iter_mut().enumerate() {
            *w = base.checked_add(i as u32).ok_or("circuit span wire overflow")?;
        }
        Ok(out)
    }
}
impl CircuitBuilderV5 {
    pub fn input_byte_span(&mut self, count: usize) -> Result<InputByteSpanV5, String> {
        let bits = count.checked_mul(8).ok_or("circuit input span bit overflow")?;
        let gates = (bits as u64).checked_mul(3).ok_or("input Booleanity gate count overflow")?;
        let assertions = self.counting.map_or(self.circuit.assertions.len() as u64, |c| c.assertions);
        if bits > self.limits.maximum_input_bits.saturating_sub(self.circuit.input_bits) as usize
            || self.gate_count().checked_add(gates).is_none_or(|n| n > u64::from(self.limits.maximum_gates))
            || assertions.checked_add(bits as u64).is_none_or(|n| n > u64::from(self.limits.maximum_assertions))
        {
            return Err("circuit input span/Booleanity constraints exceed preflight".into());
        }
        let first = u32::try_from(self.gate_count()).map_err(|_| "input span wire overflow")?;
        for _ in 0..bits {
            self.input()?;
        }
        // Explicit field-compatible b*(1-b)=0 constraints. Merely assuming a
        // PCS witness's input values are bits would not bind the native hash.
        for i in 0..bits {
            let wire =
                first.checked_add(u32::try_from(i).map_err(|_| "input bit ordinal overflow")?).ok_or("input bit wire overflow")?;
            let inverted = self.not(wire)?;
            let non_boolean = self.and(wire, inverted)?;
            self.assert(non_boolean, false)?;
        }
        Ok(InputByteSpanV5 { first, bytes: count })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn limits(cost: CircuitCostV5) -> CircuitLimitsV5 {
        CircuitLimitsV5 {
            maximum_gates: u32::try_from(cost.gates).unwrap(),
            maximum_input_bits: u32::try_from(cost.input_bits).unwrap(),
            maximum_assertions: u32::try_from(cost.assertions).unwrap(),
            maximum_graph_bytes: cost.graph_bytes,
            maximum_witness_bytes: cost.witness_bytes,
        }
    }
    #[test]
    fn native_hash_gate_graph_matches_reference_key_and_final_block_boundaries() {
        for key in [&[][..], &b"misaka-palw/circuit-test/v5"[..], &[0x55; 64][..]] {
            for bytes in [0usize, 1, 127, 128, 129] {
                let message: Vec<_> = (0..bytes).map(|i| (i as u8).wrapping_mul(7)).collect();
                let expected = crate::hash::finish(crate::hash::keyed(key).update(&message).clone());
                let cost =
                    blake2b::graph_cost_v5(bytes as u64 * 8, blake2b::compression_blocks_v5(bytes as u64, key.len()).unwrap(), 512)
                        .unwrap();
                let memory = CircuitMemoryBudgetV5::new(cost.graph_bytes + cost.witness_bytes);
                let circuit = blake2b::compile_keyed_hash_v5(key, bytes, expected, limits(cost), memory.clone()).unwrap();
                assert_eq!(circuit.circuit().cost().unwrap(), cost, "exact gate preflight");
                assert_eq!(memory.held(), cost.graph_bytes);
                let inputs = input_bits_v5(&message);
                let witness = circuit.evaluate(&inputs, limits(cost)).unwrap();
                assert_eq!(memory.held(), cost.graph_bytes + cost.witness_bytes);
                drop(witness);
                assert_eq!(memory.held(), cost.graph_bytes);
                if !inputs.is_empty() {
                    let mut bad = inputs;
                    bad[0] = !bad[0];
                    assert!(circuit.evaluate(&bad, limits(cost)).is_err());
                    assert_eq!(memory.held(), cost.graph_bytes, "failed witness releases its reservation");
                }
                drop(circuit);
                assert_eq!(memory.held(), 0);
            }
        }
    }
    #[test]
    fn native_tensor_endpoints_use_unchanged_dual_tiled_and_hiding_roots() {
        use native_commitment::*;
        for shape in [&[][..], &[0][..], &[2, 3][..], &[65][..]] {
            let count = shape.iter().copied().product::<usize>();
            let tensor = misaka_palw_tir::Tensor::new(
                misaka_palw_tir::DType::I8,
                shape.to_vec(),
                (0..count).map(|i| (i as i128 % 17) - 8).collect(),
            )
            .unwrap();
            for suite in [
                NativeTensorCommitmentV5::DualRootV2,
                NativeTensorCommitmentV5::TiledDualRootV3,
                NativeTensorCommitmentV5::HidingFlatV1,
            ] {
                let expected = match suite {
                    NativeTensorCommitmentV5::DualRootV2 => crate::merkle::tensor_commitment_v2(&tensor),
                    NativeTensorCommitmentV5::TiledDualRootV3 => crate::merkle3::tensor_commitment_v3(&tensor),
                    NativeTensorCommitmentV5::HidingFlatV1 => crate::hide::tensor_commitment_hide_v1(&tensor),
                };
                let cost = native_tensor_cost_v5(suite, tensor.dtype, &tensor.shape).unwrap();
                let memory = CircuitMemoryBudgetV5::new(cost.graph_bytes + cost.witness_bytes + 256 * 1024);
                let circuit =
                    compile_native_tensor_v5(suite, tensor.dtype, &tensor.shape, expected, limits(cost), memory.clone()).unwrap();
                assert_eq!(circuit.circuit().cost().unwrap(), cost);
                circuit.evaluate(&input_bits_v5(&tensor.to_le_bytes()), limits(cost)).unwrap();
                drop(circuit);
                assert_eq!(memory.held(), 0);
            }
        }
    }
    #[test]
    fn semantic_preflight_refuses_before_graph_and_input_capacity_are_allocated() {
        let cost = blake2b::graph_cost_v5(8, 2, 512).unwrap();
        let memory = CircuitMemoryBudgetV5::new(0);
        assert!(blake2b::compile_keyed_hash_v5(b"domain", 1, [0; 64], limits(cost), memory.clone()).is_err());
        assert_eq!(memory.held(), 0);
        let mut cap = limits(cost);
        cap.maximum_gates = 2;
        assert!(blake2b::compile_keyed_hash_v5(b"domain", 1, [0; 64], cap, memory.clone()).is_err());
        assert_eq!(memory.held(), 0);
    }
}

impl CircuitBuilderV5 {
    fn gate_count(&self) -> u64 {
        self.counting.map_or(self.circuit.gates.len() as u64, |c| c.gates)
    }
    /// Execute the same gate-emission program without allocating a graph or a
    /// witness. Operations never constant-fold, so dummy operand wires preserve
    /// exact counts; counted tensors need no per-element wire storage.
    pub fn preflight(limits: CircuitLimitsV5) -> Result<Self, String> {
        let mut builder = Self {
            circuit: BooleanCircuitV5 { version: 5, input_bits: 0, gates: Vec::new(), assertions: Vec::new() },
            limits,
            planned: None,
            counting: Some(CircuitCostV5 {
                gates: 0,
                input_bits: 0,
                assertions: 0,
                graph_bytes: 0,
                witness_bytes: 0,
                xor_gates: 0,
                and_gates: 0,
                not_gates: 0,
            }),
        };
        builder.push(BooleanGateV5::Constant(false))?;
        builder.push(BooleanGateV5::Constant(true))?;
        Ok(builder)
    }
    pub fn preflight_cost(&self) -> Result<CircuitCostV5, String> {
        self.counting.ok_or("builder is not a counting compiler".into())
    }
    pub fn is_preflight(&self) -> bool {
        self.counting.is_some()
    }
}
