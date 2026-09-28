//! Strided views: a tensor is a run of typed storage plus a [`Layout`] — shape, strides and an
//! offset. The structural primitives (`Reshape` of a contiguous value, `Transpose`, `Slice`,
//! `Broadcast`, a scalar-index `Gather`, `HistAppend`'s window) are layouts over their input's
//! storage, so a transposed weight matrix, a KV window or an embedding row is never copied.
//!
//! Strides are element counts and never negative (no v1 primitive reverses an axis); a stride of 0
//! is a broadcast dimension.

// Index loops over the fixed-rank arrays read as the definitions they implement.
#![allow(clippy::needless_range_loop)]

/// Largest rank (spec 04b §2.2).
pub const MAX_RANK: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub rank: u8,
    pub shape: [usize; MAX_RANK],
    pub strides: [usize; MAX_RANK],
    pub offset: usize,
}

/// Row-major strides of `shape`.
pub fn row_major(shape: &[usize]) -> [usize; MAX_RANK] {
    let mut s = [0usize; MAX_RANK];
    let mut acc = 1usize;
    for i in (0..shape.len()).rev() {
        s[i] = acc;
        acc *= shape[i];
    }
    s
}

pub fn numel(shape: &[usize]) -> usize {
    shape.iter().product()
}

impl Layout {
    /// A contiguous row-major layout starting at `offset`.
    pub fn contiguous_at(shape: &[usize], offset: usize) -> Self {
        debug_assert!(shape.len() <= MAX_RANK);
        let mut sh = [1usize; MAX_RANK];
        sh[..shape.len()].copy_from_slice(shape);
        Layout { rank: shape.len() as u8, shape: sh, strides: row_major(shape), offset }
    }

    pub fn contiguous(shape: &[usize]) -> Self {
        Self::contiguous_at(shape, 0)
    }

    #[inline]
    pub fn shape(&self) -> &[usize] {
        &self.shape[..self.rank as usize]
    }

    #[inline]
    pub fn strides(&self) -> &[usize] {
        &self.strides[..self.rank as usize]
    }

    #[inline]
    pub fn numel(&self) -> usize {
        numel(self.shape())
    }

    /// Does a row-major walk visit `offset, offset + 1, …` in order?
    pub fn is_contiguous(&self) -> bool {
        let mut expect = 1usize;
        for i in (0..self.rank as usize).rev() {
            if self.shape[i] != 1 && self.strides[i] != expect {
                return false;
            }
            expect *= self.shape[i];
        }
        true
    }

    /// One past the largest storage index the layout touches (0 for no element — never, since
    /// every dimension is at least 1).
    pub fn extent(&self) -> usize {
        self.offset + self.shape().iter().zip(self.strides()).map(|(d, s)| (d - 1) * s).sum::<usize>() + 1
    }

    /// `out.shape[i] = shape[perm[i]]`.
    pub fn transposed(&self, perm: &[u8]) -> Self {
        let mut l = *self;
        for (i, p) in perm.iter().enumerate() {
            l.shape[i] = self.shape[*p as usize];
            l.strides[i] = self.strides[*p as usize];
        }
        l
    }

    /// `len` elements of `axis` from `start`.
    pub fn sliced(&self, axis: usize, start: usize, len: usize) -> Self {
        let mut l = *self;
        l.offset += start * self.strides[axis];
        l.shape[axis] = len;
        l
    }

    /// Numpy broadcast to `out` (the type rule guarantees compatibility).
    pub fn broadcast_to(&self, out: &[usize]) -> Self {
        let (ri, ro) = (self.rank as usize, out.len());
        let mut l = Layout { rank: ro as u8, shape: [1; MAX_RANK], strides: [0; MAX_RANK], offset: self.offset };
        for i in 0..ro {
            l.shape[i] = out[i];
            if i + ri >= ro {
                let k = i + ri - ro;
                l.strides[i] = if self.shape[k] == 1 && out[i] != 1 { 0 } else { self.strides[k] };
            }
        }
        l
    }

    /// A new shape over the same storage; only for a contiguous layout.
    pub fn reshaped(&self, shape: &[usize]) -> Self {
        debug_assert!(self.is_contiguous());
        Self::contiguous_at(shape, self.offset)
    }

    /// Drop `axis` (a gather by a scalar index along it, after the offset moved).
    pub fn without_axis(&self, axis: usize) -> Self {
        let r = self.rank as usize;
        let mut l = Layout { rank: (r - 1) as u8, shape: [1; MAX_RANK], strides: [0; MAX_RANK], offset: self.offset };
        let mut k = 0;
        for i in 0..r {
            if i != axis {
                l.shape[k] = self.shape[i];
                l.strides[k] = self.strides[i];
                k += 1;
            }
        }
        l
    }

    /// Visit the storage in row-major order as maximal runs `(start, len, stride)`.
    pub fn for_each_run(&self, mut f: impl FnMut(usize, usize, usize)) {
        let j = Joint::<1>::new(self.shape(), [self.strides()]);
        j.for_each(|_, [base], len, [stride]| f(self.offset + base, len, stride));
    }
}

/// A joint row-major walk of one index space by several strided operands, with adjacent
/// dimensions merged wherever every operand allows it, so the inner loop is as long as possible.
pub struct Joint<const N: usize> {
    nd: usize,
    shape: [usize; MAX_RANK],
    strides: [[usize; MAX_RANK]; N],
}

impl<const N: usize> Joint<N> {
    /// `strides[k]` are operand `k`'s strides over `shape` (same rank; 0 for broadcast).
    pub fn new(shape: &[usize], strides: [&[usize]; N]) -> Self {
        // Dimensions of extent 1 never move an index; drop them.
        let mut dims: Vec<(usize, [usize; N])> = Vec::with_capacity(MAX_RANK);
        for i in 0..shape.len() {
            if shape[i] != 1 {
                let mut s = [0usize; N];
                for k in 0..N {
                    s[k] = strides[k][i];
                }
                dims.push((shape[i], s));
            }
        }
        // Merge an outer dimension into the next inner one when, for every operand, the outer
        // stride is the inner stride times the inner extent.
        let mut merged: Vec<(usize, [usize; N])> = Vec::with_capacity(dims.len());
        for (d, s) in dims {
            if let Some((pd, ps)) = merged.last_mut()
                && (0..N).all(|k| ps[k] == s[k] * d)
            {
                *pd *= d;
                *ps = s;
                continue;
            }
            merged.push((d, s));
        }
        let mut j = Joint { nd: merged.len(), shape: [1; MAX_RANK], strides: [[0; MAX_RANK]; N] };
        for (i, (d, s)) in merged.into_iter().enumerate() {
            j.shape[i] = d;
            for k in 0..N {
                j.strides[k][i] = s[k];
            }
        }
        j
    }

    /// `f(run_index, operand_bases, len, operand_inner_strides)` for every inner run, in row-major
    /// order; `run_index` is the row-major position of the run's first element.
    #[inline]
    pub fn for_each(&self, mut f: impl FnMut(usize, [usize; N], usize, [usize; N])) {
        if self.nd == 0 {
            f(0, [0; N], 1, [0; N]);
            return;
        }
        let inner = self.nd - 1;
        let len = self.shape[inner];
        let mut inner_strides = [0usize; N];
        for k in 0..N {
            inner_strides[k] = self.strides[k][inner];
        }
        let outer: usize = self.shape[..inner].iter().product();
        let mut idx = [0usize; MAX_RANK];
        let mut base = [0usize; N];
        let mut pos = 0usize;
        for _ in 0..outer {
            f(pos, base, len, inner_strides);
            pos += len;
            // Odometer over the outer dimensions.
            let mut d = inner;
            while d > 0 {
                d -= 1;
                idx[d] += 1;
                for k in 0..N {
                    base[k] += self.strides[k][d];
                }
                if idx[d] < self.shape[d] {
                    break;
                }
                for k in 0..N {
                    base[k] -= self.strides[k][d] * self.shape[d];
                }
                idx[d] = 0;
            }
        }
    }
}

/// Copy the elements a strided layout names, in row-major order, converting each.
#[inline]
pub fn gather_strided<S: Copy, D>(src: &[S], layout: &Layout, dst: &mut Vec<D>, conv: impl Fn(S) -> D) {
    dst.clear();
    dst.reserve(layout.numel());
    layout.for_each_run(|start, len, stride| {
        if stride == 1 {
            dst.extend(src[start..start + len].iter().map(|v| conv(*v)));
        } else {
            dst.extend((0..len).map(|i| conv(src[start + i * stride])));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walk(l: &Layout) -> Vec<usize> {
        let mut out = Vec::new();
        l.for_each_run(|s, n, st| out.extend((0..n).map(|i| s + i * st)));
        out
    }

    fn naive(l: &Layout) -> Vec<usize> {
        let sh = l.shape();
        let n = numel(sh);
        let rm = row_major(sh);
        (0..n)
            .map(|lin| {
                let mut off = l.offset;
                for (d, st) in rm[..sh.len()].iter().enumerate() {
                    off += (lin / st) % sh[d] * l.strides[d];
                }
                off
            })
            .collect()
    }

    #[test]
    fn walks_equal_the_definition_for_views() {
        let base = Layout::contiguous(&[2, 3, 4]);
        for l in [
            base,
            base.transposed(&[2, 0, 1]),
            base.transposed(&[1, 2, 0]).sliced(1, 1, 2),
            base.sliced(2, 1, 3),
            Layout::contiguous(&[3, 1]).broadcast_to(&[2, 3, 5]),
            Layout::contiguous(&[1]).broadcast_to(&[4]),
            Layout::contiguous_at(&[], 7),
            base.without_axis(1),
        ] {
            assert_eq!(walk(&l), naive(&l), "{l:?}");
            assert_eq!(l.is_contiguous(), naive(&l).iter().enumerate().all(|(i, o)| *o == l.offset + i), "{l:?}");
        }
    }
}
