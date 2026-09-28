//! # `misaka-palw-tir-ref2` — the independent second implementation of PALW-TIR v1
//!
//! Written from `docs/spec/palw/04b-tensor-ir.md` alone (RFC-0002 freeze criterion 4:
//! `reference evaluator == independent second implementation == every backend`). It never reads
//! the first implementation (`misaka-palw-tir`); its tests use that crate only as a black box.
//! Where the text is silent or ambiguous, the reading taken here is marked in the code and argued
//! in `docs/design/palw/tir/ref2-findings.md`.
//!
//! Test/verification-only — never a consensus dependency.
//!
//! Structure, deliberately unlike the obvious one:
//! - [`codec`]: the §4 encoding by hand (no derive), decoding and re-encoding;
//! - [`wide`]: every exact result as a 256-bit signed integer, range-checked afterwards;
//! - [`transcendental`]: `>>` as floor division by the definition, constants re-declared;
//! - [`prims`]: index maps by unravel/ravel of §0, one output element at a time;
//! - [`eval`]: a step as a pure function of the run state.

pub mod codec;
pub mod error;
pub mod eval;
pub mod normal_form;
pub mod prims;
pub mod program;
pub mod tensor;
pub mod transcendental;
pub mod types;
pub mod typing;
pub mod wide;

pub use error::{Class, Res, TirError};
pub use program::*;
pub use tensor::Tensor;
pub use types::{DType, Dim, TensorType};

/// Evaluates one stateless primitive on concrete operands with a concrete output type, as the
/// primitive golden vectors (§12) require: the type rule of §6 on the operand and output types
/// (all dimensions `Fixed`), then the value.
pub fn eval_primitive(prim: &Prim, ins: &[Tensor], out_dtype: DType, out_shape: &[u64]) -> Res<Tensor> {
    if matches!(prim, Prim::StateWrite { .. } | Prim::HistAppend { .. }) {
        return Err(TirError::new(Class::Shape, "state primitives need a program"));
    }
    let as_type = |dtype: DType, shape: &[u64]| -> Res<TensorType> {
        let mut dims = Vec::with_capacity(shape.len());
        for &d in shape {
            if d == 0 || d > u32::MAX as u64 {
                return Err(TirError::new(Class::Shape, format!("dimension {d}")));
            }
            dims.push(Dim::Fixed(d as u32));
        }
        Ok(TensorType { dtype, shape: dims })
    };
    let mut in_types = Vec::with_capacity(ins.len());
    for t in ins {
        if tensor::count(&t.shape) != t.data.len() as u64 || t.data.iter().any(|&v| !t.dtype.contains(v)) {
            return Err(TirError::new(Class::Operand, "malformed operand"));
        }
        in_types.push(as_type(t.dtype, &t.shape)?);
    }
    let out = as_type(out_dtype, out_shape)?;
    out.check_legal(None)?;
    typing::check_type(prim, &in_types, &out, &[])?;
    let refs: Vec<&Tensor> = ins.iter().collect();
    prims::eval_prim(prim, &refs, out_dtype, out_shape, &[], None)
}
