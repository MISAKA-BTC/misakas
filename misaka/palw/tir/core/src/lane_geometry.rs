//! Pure row-major primitive coordinate maps. No values or execution authority.
//! Shared with the original Demand evaluator; producer tracing may use captured
//! control values, while Node continues to compute its own values independently.
use crate::types::strides;
pub fn unravel(mut i: usize, st: &[usize]) -> Vec<usize> {
    st.iter()
        .map(|s| {
            let q = i / s;
            i %= s;
            q
        })
        .collect()
}

pub fn ravel(ix: &[usize], st: &[usize]) -> usize {
    ix.iter().zip(st).map(|(a, b)| a * b).sum()
}

/// The element of `in_shape` that output multi-index `o` (of rank ≥ `in_shape.len()`) reads under
/// numpy broadcasting — `eval.rs`'s `broadcast_map`, one element at a time.
pub fn broadcast_index(o: &[usize], in_shape: &[usize]) -> usize {
    let ist = strides(in_shape);
    let off = o.len() - in_shape.len();
    (0..in_shape.len()).map(|k| if in_shape[k] == 1 { 0 } else { o[off + k] * ist[k] }).sum()
}

pub fn gather_index_coordinate(o: &[usize], axis: usize, batch: usize, index_rank: usize) -> Vec<usize> {
    let m = index_rank - batch;
    let mut ix = o[..batch].to_vec();
    ix.extend_from_slice(&o[axis..axis + m]);
    ix
}
pub fn gather_data_coordinate(o: &[usize], axis: usize, index_rank: usize, batch: usize, selected: usize) -> Vec<usize> {
    let m = index_rank - batch;
    let mut ix = o[..axis].to_vec();
    ix.push(selected);
    ix.extend_from_slice(&o[axis + m..]);
    ix
}
