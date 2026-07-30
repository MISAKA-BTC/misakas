//! # PalwComputeIR V1 — the bounded, versioned integer instruction set (§15)
//!
//! A model's semantic program is DATA lowered onto this fixed opcode set; the node implements
//! ONE generic validator for programs, never a per-model verifier (ADR-MA-001/006). The safety
//! posture is §15.2 and §23.5, enforced structurally:
//!
//! * **No arbitrary code**: an instruction is `(opcode, value inputs, bounded attribute bytes)`.
//!   There are no syscalls, no host imports, no native hooks to name.
//! * **Acyclic by construction**: instruction `i` may only read tensor slots or the outputs of
//!   instructions `< i`. A cycle or unbounded loop is UNREPRESENTABLE, not merely rejected.
//! * **Statically bounded**: instruction count, tensor slots, per-instruction fan-in and
//!   attribute bytes are all capped by [`PalwComputeIrLimitsV1`] before anything interprets the
//!   program.
//! * **Pinned wire form**: opcodes are explicit `u16` codes (never borsh positional indexes);
//!   unknown codes fail decode — fail-closed, no default op (§22.3).
//!
//! `compute_vm_id` (§15.4) freezes the VM surface: the opcode table, the arithmetic-semantics
//! tag and the canonical-encoding tag. A model needing an op outside this table is a **Compute
//! VM upgrade** (V2 proposal + activation), never a silent extension of V1.

use crate::palw_compute_set::PALW_COMPUTE_VM_ID_DOMAIN;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{Hash64, blake2b_512_keyed};
use thiserror::Error;

/// Keyed domain for a canonical Compute IR program root (`descriptor.semantic_program_root`).
pub const PALW_COMPUTE_IR_PROGRAM_DOMAIN: &[u8] = b"misaka-palw-compute-ir-prog-v1";

pub const PALW_COMPUTE_IR_PROGRAM_VERSION: u16 = 1;

/// §15.4 — the frozen V1 arithmetic-semantics tag: deterministic two's-complement integer ops,
/// explicit overflow budgets, round-to-nearest-even shifts (the QI35 integer-class rules).
pub const PALW_COMPUTE_VM_V1_ARITHMETIC_TAG: &str = "int-deterministic/rne/overflow-budgeted/v1";

/// §15.4 — the frozen V1 canonical-encoding tag (borsh, little-endian, explicit discriminants).
pub const PALW_COMPUTE_VM_V1_ENCODING_TAG: &str = "borsh-le/explicit-discriminants/v1";

// =============================================================================================
// §15.3 — the V1 opcode table (explicit codes; the wire form is the plain u16)
// =============================================================================================

macro_rules! compute_ir_opcodes {
    ($(($variant:ident, $code:expr, $min_inputs:expr, $max_inputs:expr)),+ $(,)?) => {
        /// The V1 opcode set (§15.3). Codes are part of the frozen VM surface — NEVER renumber;
        /// additions require a new VM version (§15.4/ADR-MA-006).
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        #[repr(u16)]
        pub enum PalwComputeIrOpcode {
            $($variant = $code),+
        }

        impl PalwComputeIrOpcode {
            pub const ALL: &'static [PalwComputeIrOpcode] = &[$(Self::$variant),+];

            #[inline]
            pub const fn as_u16(self) -> u16 {
                self as u16
            }

            pub const fn from_u16(v: u16) -> Option<Self> {
                match v {
                    $($code => Some(Self::$variant),)+
                    _ => None,
                }
            }

            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => stringify!($variant)),+
                }
            }

            /// Structural fan-in bounds per op — a first-line arity sanity check (§15.2's
            /// bounded-rank discipline; full shape checking rides the shape tables).
            pub const fn input_arity(self) -> (usize, usize) {
                match self {
                    $(Self::$variant => ($min_inputs, $max_inputs)),+
                }
            }
        }
    };
}

compute_ir_opcodes![
    (Int4Unpack, 1, 1, 1),
    (Int8Load, 2, 1, 1),
    (Int8Gemm, 3, 2, 3),
    (Int32Reduce, 4, 1, 2),
    (Int64Requantize, 5, 1, 2),
    (RneShift, 6, 1, 1),
    (Clamp, 7, 1, 1),
    (Add, 8, 2, 2),
    (Subtract, 9, 2, 2),
    (Multiply, 10, 2, 2),
    (DivideFloor, 11, 2, 2),
    (IntegerSqrt, 12, 1, 1),
    (Gather, 13, 2, 2),
    (Scatter, 14, 3, 3),
    (TopK, 15, 1, 2),
    (StableSort, 16, 1, 2),
    (LookupTable, 17, 2, 2),
    (RmsNormFixed, 18, 1, 3),
    (SoftmaxFixed, 19, 1, 2),
    (RoPeFixed, 20, 1, 3),
    (ActivationFixed, 21, 1, 2),
    (StateRead, 22, 1, 2),
    (StateWrite, 23, 2, 3),
    (KvRead, 24, 1, 2),
    (KvWrite, 25, 2, 3),
    (ConvInteger, 26, 2, 4),
    (PatchEmbeddingInteger, 27, 2, 3),
    (ResidualAdd, 28, 2, 2),
    (ExpertDispatch, 29, 2, 3),
    (ExpertMerge, 30, 2, 4),
    (MerkleCheckpoint, 31, 1, 8),
];

impl BorshSerialize for PalwComputeIrOpcode {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> borsh::io::Result<()> {
        self.as_u16().serialize(writer)
    }
}

impl BorshDeserialize for PalwComputeIrOpcode {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> borsh::io::Result<Self> {
        let raw = u16::deserialize_reader(reader)?;
        PalwComputeIrOpcode::from_u16(raw)
            .ok_or_else(|| borsh::io::Error::new(borsh::io::ErrorKind::InvalidData, format!("unknown Compute IR opcode {raw}")))
    }
}

// =============================================================================================
// §15.4 — the frozen VM identity
// =============================================================================================

/// The canonical V1 VM surface: opcode table (code + name + arity) plus the two semantics tags.
/// This string IS the `compute_vm_id` preimage — pinned verbatim by tests, because changing one
/// byte here forks which programs a node accepts.
pub fn compute_vm_v1_surface() -> String {
    let mut surface = format!(
        "palw-compute-vm/v1\narithmetic={PALW_COMPUTE_VM_V1_ARITHMETIC_TAG}\nencoding={PALW_COMPUTE_VM_V1_ENCODING_TAG}\n"
    );
    for op in PalwComputeIrOpcode::ALL {
        let (min, max) = op.input_arity();
        surface.push_str(&format!("op {} {} {} {}\n", op.as_u16(), op.name(), min, max));
    }
    surface
}

/// §15.4 — `compute_vm_id = Hash64_k(compute-vm-id, canonical VM surface)`. A descriptor names
/// this id; a node that does not implement it rejects the set (§22.3), it never approximates.
pub fn compute_vm_id_v1() -> Hash64 {
    blake2b_512_keyed(PALW_COMPUTE_VM_ID_DOMAIN, compute_vm_v1_surface().as_bytes())
}

// =============================================================================================
// Program container + static validation
// =============================================================================================

/// One instruction: an opcode, its value inputs, and bounded opaque attribute bytes (shape ids,
/// LUT selectors, axis constants… — interpreted per-op by the VM, never executed).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwComputeIrInstructionV1 {
    pub opcode: PalwComputeIrOpcode,
    /// Value references: `0..tensor_count` are input tensor slots; `tensor_count + j` is the
    /// output of instruction `j`. Validation requires every reference to point STRICTLY earlier,
    /// which makes the graph acyclic by construction (§15.2).
    pub inputs: Vec<u32>,
    pub attributes: Vec<u8>,
}

/// A complete semantic program: the payload behind `descriptor.semantic_program_root`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwComputeIrProgramV1 {
    pub version: u16,
    /// The VM whose surface this program was validated against (must equal the descriptor's).
    pub compute_vm_id: Hash64,
    /// Number of externally provided tensor slots (weights, activations, state handles).
    pub tensor_count: u32,
    pub instructions: Vec<PalwComputeIrInstructionV1>,
}

impl PalwComputeIrProgramV1 {
    /// Content root committed by `descriptor.semantic_program_root`.
    pub fn program_root(&self) -> Hash64 {
        blake2b_512_keyed(PALW_COMPUTE_IR_PROGRAM_DOMAIN, &borsh::to_vec(self).expect("borsh"))
    }
}

/// §15.2/§23.5 — the static resource bounds a program is validated against BEFORE any
/// interpretation. Governed per network; defaults are the v0.1 initial numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwComputeIrLimitsV1 {
    pub max_instructions: u32,
    pub max_tensors: u32,
    pub max_inputs_per_instruction: u32,
    pub max_attribute_bytes_per_instruction: u32,
    pub max_total_attribute_bytes: u64,
}

impl Default for PalwComputeIrLimitsV1 {
    fn default() -> Self {
        Self {
            max_instructions: 1_048_576,
            max_tensors: 262_144,
            max_inputs_per_instruction: 64,
            max_attribute_bytes_per_instruction: 4_096,
            max_total_attribute_bytes: 16 * 1024 * 1024,
        }
    }
}

/// Static whole-program validation (§15.2). Everything here is O(program size); nothing
/// interprets, allocates tensors, or touches model data.
pub fn validate_compute_ir_program(
    program: &PalwComputeIrProgramV1,
    limits: &PalwComputeIrLimitsV1,
) -> Result<(), ComputeIrError> {
    if program.version != PALW_COMPUTE_IR_PROGRAM_VERSION {
        return Err(ComputeIrError::UnsupportedProgramVersion(program.version));
    }
    // §22.3 `unsupported VM -> reject`: V1 validation only vouches for the V1 surface.
    if program.compute_vm_id != compute_vm_id_v1() {
        return Err(ComputeIrError::UnsupportedVm(program.compute_vm_id));
    }
    if program.instructions.is_empty() {
        return Err(ComputeIrError::EmptyProgram);
    }
    if program.instructions.len() as u64 > limits.max_instructions as u64 {
        return Err(ComputeIrError::TooManyInstructions { count: program.instructions.len() as u64, max: limits.max_instructions });
    }
    if program.tensor_count > limits.max_tensors {
        return Err(ComputeIrError::TooManyTensors { count: program.tensor_count, max: limits.max_tensors });
    }
    let mut total_attribute_bytes: u64 = 0;
    for (index, instruction) in program.instructions.iter().enumerate() {
        let (min_inputs, max_inputs) = instruction.opcode.input_arity();
        if instruction.inputs.len() < min_inputs || instruction.inputs.len() > max_inputs {
            return Err(ComputeIrError::ArityViolation {
                index: index as u32,
                opcode: instruction.opcode,
                inputs: instruction.inputs.len() as u32,
            });
        }
        if instruction.inputs.len() as u32 > limits.max_inputs_per_instruction {
            return Err(ComputeIrError::TooManyInputs { index: index as u32, count: instruction.inputs.len() as u32 });
        }
        if instruction.attributes.len() as u32 > limits.max_attribute_bytes_per_instruction {
            return Err(ComputeIrError::AttributesTooLarge { index: index as u32, bytes: instruction.attributes.len() as u64 });
        }
        total_attribute_bytes += instruction.attributes.len() as u64;
        // Acyclic-by-construction: only tensor slots and STRICTLY EARLIER instruction outputs
        // are addressable. A forward or self reference is a malformed program, so loops and
        // recursion are unrepresentable rather than detected (§15.2, §23.5).
        let addressable = program.tensor_count as u64 + index as u64;
        for &value_ref in &instruction.inputs {
            if (value_ref as u64) >= addressable {
                return Err(ComputeIrError::ForwardOrSelfReference { index: index as u32, value_ref });
            }
        }
    }
    if total_attribute_bytes > limits.max_total_attribute_bytes {
        return Err(ComputeIrError::TotalAttributesTooLarge { bytes: total_attribute_bytes, max: limits.max_total_attribute_bytes });
    }
    Ok(())
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ComputeIrError {
    #[error("program version {0} is not supported (expected {PALW_COMPUTE_IR_PROGRAM_VERSION})")]
    UnsupportedProgramVersion(u16),

    #[error("program targets unsupported compute VM {0} — this node implements V1 only (§22.3 fail-closed)")]
    UnsupportedVm(Hash64),

    #[error("program has no instructions")]
    EmptyProgram,

    #[error("{count} instructions exceed the {max} cap (§15.2)")]
    TooManyInstructions { count: u64, max: u32 },

    #[error("{count} tensor slots exceed the {max} cap (§15.2)")]
    TooManyTensors { count: u32, max: u32 },

    #[error("instruction {index}: {opcode:?} fan-in {inputs} outside its arity bounds")]
    ArityViolation { index: u32, opcode: PalwComputeIrOpcode, inputs: u32 },

    #[error("instruction {index}: {count} inputs exceed the per-instruction cap")]
    TooManyInputs { index: u32, count: u32 },

    #[error("instruction {index}: attribute payload of {bytes} bytes exceeds the per-instruction cap")]
    AttributesTooLarge { index: u32, bytes: u64 },

    #[error("total attribute payload of {bytes} bytes exceeds the {max}-byte program cap")]
    TotalAttributesTooLarge { bytes: u64, max: u64 },

    #[error("instruction {index}: value reference {value_ref} is not strictly earlier — cycles are unrepresentable (§15.2)")]
    ForwardOrSelfReference { index: u32, value_ref: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single_op_program() -> PalwComputeIrProgramV1 {
        PalwComputeIrProgramV1 {
            version: PALW_COMPUTE_IR_PROGRAM_VERSION,
            compute_vm_id: compute_vm_id_v1(),
            tensor_count: 2,
            instructions: vec![PalwComputeIrInstructionV1 {
                opcode: PalwComputeIrOpcode::Int8Gemm,
                inputs: vec![0, 1],
                attributes: vec![],
            }],
        }
    }

    #[test]
    fn opcode_codes_are_pinned_and_fail_closed() {
        use PalwComputeIrOpcode::*;
        // The complete frozen V1 table (§15.3): 31 ops, codes 1..=31 in document order.
        let pinned: [(PalwComputeIrOpcode, u16); 31] = [
            (Int4Unpack, 1),
            (Int8Load, 2),
            (Int8Gemm, 3),
            (Int32Reduce, 4),
            (Int64Requantize, 5),
            (RneShift, 6),
            (Clamp, 7),
            (Add, 8),
            (Subtract, 9),
            (Multiply, 10),
            (DivideFloor, 11),
            (IntegerSqrt, 12),
            (Gather, 13),
            (Scatter, 14),
            (TopK, 15),
            (StableSort, 16),
            (LookupTable, 17),
            (RmsNormFixed, 18),
            (SoftmaxFixed, 19),
            (RoPeFixed, 20),
            (ActivationFixed, 21),
            (StateRead, 22),
            (StateWrite, 23),
            (KvRead, 24),
            (KvWrite, 25),
            (ConvInteger, 26),
            (PatchEmbeddingInteger, 27),
            (ResidualAdd, 28),
            (ExpertDispatch, 29),
            (ExpertMerge, 30),
            (MerkleCheckpoint, 31),
        ];
        assert_eq!(PalwComputeIrOpcode::ALL.len(), pinned.len());
        for (op, code) in pinned {
            assert_eq!(op.as_u16(), code);
            assert_eq!(PalwComputeIrOpcode::from_u16(code), Some(op));
            assert_eq!(borsh::to_vec(&op).unwrap(), code.to_le_bytes().to_vec());
        }
        // Unknown opcodes fail decode — never a default op (§22.3).
        assert_eq!(PalwComputeIrOpcode::from_u16(0), None);
        assert_eq!(PalwComputeIrOpcode::from_u16(32), None);
        assert!(borsh::from_slice::<PalwComputeIrOpcode>(&32u16.to_le_bytes()).is_err());
    }

    #[test]
    fn vm_surface_is_frozen() {
        let surface = compute_vm_v1_surface();
        assert!(surface.starts_with("palw-compute-vm/v1\narithmetic=int-deterministic/rne/overflow-budgeted/v1\n"));
        assert!(surface.contains("op 3 Int8Gemm 2 3\n"));
        assert!(surface.contains("op 31 MerkleCheckpoint 1 8\n"));
        assert_eq!(surface.lines().count(), 3 + 31, "opcode table length is part of the frozen surface");
        // The id is a pure function of the surface — recomputation is bit-stable.
        assert_eq!(compute_vm_id_v1(), compute_vm_id_v1());
        assert_ne!(compute_vm_id_v1(), Hash64::default());
    }

    #[test]
    fn valid_program_passes_and_roots_are_content_derived() {
        let limits = PalwComputeIrLimitsV1::default();
        let program = single_op_program();
        assert_eq!(validate_compute_ir_program(&program, &limits), Ok(()));
        let mut changed = program.clone();
        changed.instructions[0].attributes = vec![1];
        assert_ne!(program.program_root(), changed.program_root());
    }

    #[test]
    fn acyclicity_is_structural() {
        let limits = PalwComputeIrLimitsV1::default();
        // Self reference: instruction 0 reading value tensor_count+0 (its own output).
        let mut self_ref = single_op_program();
        self_ref.instructions[0].inputs = vec![0, 2];
        assert!(matches!(
            validate_compute_ir_program(&self_ref, &limits),
            Err(ComputeIrError::ForwardOrSelfReference { index: 0, value_ref: 2 })
        ));
        // Forward reference: instruction 0 reading instruction 1's output.
        let mut forward = single_op_program();
        forward.instructions = vec![
            PalwComputeIrInstructionV1 { opcode: PalwComputeIrOpcode::Add, inputs: vec![0, 3], attributes: vec![] },
            PalwComputeIrInstructionV1 { opcode: PalwComputeIrOpcode::Add, inputs: vec![0, 1], attributes: vec![] },
        ];
        assert!(matches!(
            validate_compute_ir_program(&forward, &limits),
            Err(ComputeIrError::ForwardOrSelfReference { index: 0, value_ref: 3 })
        ));
        // Backward reference is fine: instruction 1 consumes instruction 0's output (value 2).
        let mut chain = single_op_program();
        chain.instructions = vec![
            PalwComputeIrInstructionV1 { opcode: PalwComputeIrOpcode::Add, inputs: vec![0, 1], attributes: vec![] },
            PalwComputeIrInstructionV1 { opcode: PalwComputeIrOpcode::RneShift, inputs: vec![2], attributes: vec![] },
        ];
        assert_eq!(validate_compute_ir_program(&chain, &limits), Ok(()));
    }

    #[test]
    fn bounds_are_enforced() {
        let limits = PalwComputeIrLimitsV1 { max_instructions: 1, max_attribute_bytes_per_instruction: 4, ..Default::default() };
        let mut oversized = single_op_program();
        oversized.instructions.push(PalwComputeIrInstructionV1 {
            opcode: PalwComputeIrOpcode::RneShift,
            inputs: vec![2],
            attributes: vec![],
        });
        assert!(matches!(
            validate_compute_ir_program(&oversized, &limits),
            Err(ComputeIrError::TooManyInstructions { count: 2, max: 1 })
        ));
        let mut fat = single_op_program();
        fat.instructions[0].attributes = vec![0; 5];
        assert!(matches!(validate_compute_ir_program(&fat, &limits), Err(ComputeIrError::AttributesTooLarge { .. })));
        let mut wrong_arity = single_op_program();
        wrong_arity.instructions[0].inputs = vec![0]; // Int8Gemm needs ≥ 2
        assert!(matches!(validate_compute_ir_program(&wrong_arity, &limits), Err(ComputeIrError::ArityViolation { .. })));
        let mut alien_vm = single_op_program();
        alien_vm.compute_vm_id = Hash64::from_bytes([9; 64]);
        assert!(matches!(validate_compute_ir_program(&alien_vm, &limits), Err(ComputeIrError::UnsupportedVm(_))));
        let mut empty = single_op_program();
        empty.instructions.clear();
        assert!(matches!(validate_compute_ir_program(&empty, &limits), Err(ComputeIrError::EmptyProgram)));
    }
}
