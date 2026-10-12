//! **The integer params a lowering produces**, by name, and their binding to a program's param indices.
//!
//! A lowerer declares a param on the program builder (`ProgramBuilder::param`) and puts its values here under the
//! same name; once the program is finished (and, for a version-2 stage, lifted: lifting removes the declared
//! inputs and shifts the rest) [`ParamSink::bind`] maps names to the program's own indices, which is the
//! `MapParams` a run reads. A per-layer param keeps one tensor per layer.

use std::collections::BTreeMap;

use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::{DType, MapParams, Ref, Tensor};

/// Params by `(name, layer)`.
#[derive(Clone, Debug, Default)]
pub struct ParamSink {
    tensors: BTreeMap<(String, Option<u16>), Tensor>,
}

fn tensor_of(name: &str, dtype: DType, shape: &[u32], values: Vec<i128>) -> Tensor {
    Tensor::new(dtype, shape.iter().map(|d| *d as usize).collect(), values).unwrap_or_else(|e| panic!("param {name}: {e}"))
}

impl ParamSink {
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare a global param on `pb` and keep its values (its length is the shape's, every value its dtype's).
    pub fn put(&mut self, pb: &mut ProgramBuilder, name: &str, dtype: DType, shape: &[u32], values: Vec<i128>) -> Ref {
        let r = pb.param(name, dtype, shape, false);
        self.tensors.insert((name.to_string(), None), tensor_of(name, dtype, shape, values));
        r
    }

    /// Declare a per-layer param on `pb` and keep one tensor per layer (`values[l]` is layer `l`'s).
    pub fn put_layers(&mut self, pb: &mut ProgramBuilder, name: &str, dtype: DType, shape: &[u32], values: Vec<Vec<i128>>) -> Ref {
        let r = pb.param(name, dtype, shape, true);
        for (l, v) in values.into_iter().enumerate() {
            self.tensors.insert((name.to_string(), Some(l as u16)), tensor_of(name, dtype, shape, v));
        }
        r
    }

    /// One param's tensor.
    pub fn get(&self, name: &str, layer: Option<u16>) -> Option<&Tensor> {
        self.tensors.get(&(name.to_string(), layer))
    }

    /// How many elements (of any dtype) the sink holds, and how many bytes they take as stored.
    pub fn size(&self) -> (usize, usize) {
        let n = self.tensors.values().map(|t| t.data.len()).sum();
        let bytes = self.tensors.values().map(|t| t.data.len() * t.dtype.width()).sum();
        (n, bytes)
    }

    /// The `MapParams` of a program whose params are named `names` in index order: every tensor under a name the
    /// program declares, at its index. A name the program declares and the sink lacks is a lowering bug.
    pub fn bind<'a>(&self, names: impl IntoIterator<Item = &'a str>) -> MapParams {
        let mut out = MapParams::default();
        for (j, name) in names.into_iter().enumerate() {
            let mut found = false;
            for ((n, layer), t) in &self.tensors {
                if n == name {
                    out.tensors.insert((j as u16, *layer), t.clone());
                    found = true;
                }
            }
            assert!(found, "the sink has no param {name:?} (declared at index {j})");
        }
        out
    }
}
