//! Bounded owned recurrence matrix. This auxiliary tree starts at deterministic ZERO and
//! changes only through exact kernel transitions. It never replaces an original step or
//! artifact commitment; the outer court must authenticate all five public input rows and
//! registered parameter bytes before calling this internal transition engine.
//! Column pages are fixed4096 i32 lanes, with canonical zero padding. Full512-bit hashes
//! and exact path positions bind pages; one partial page has no alternate encoding.
use crate::Hash64;
use crate::palw_base0::{K, ONE, rounding_shift_right_64};
use crate::palw_base0_a16::{A16_CODE_MAX, A16QuantParams, a16_scale_round};
use crate::palw_qwen36_ops::{QWEN36_DELTA_MAX, QWEN36_STATE_MAX, QWEN36_STATE_MULT_MAX};
use borsh::{BorshDeserialize, BorshSerialize};
pub const PALW_GDN_PAGE_WORDS_V1: usize = 4096;
const LEAF: &[u8] = b"misaka-palw/prefix-gdn/owned-page/v1";
const NODE: &[u8] = b"misaka-palw/prefix-gdn/owned-node/v1";
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwGdnOwnedPageOpeningV1 {
    pub values: Vec<i32>,
    /// Bottom-up exact depth; identity/index comes from the owned cursor.
    pub siblings: Vec<Hash64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
enum PhaseV1 {
    Idle,
    Read,
    Write,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwGdnPagedStateV1 {
    context: Hash64,
    prefix_tokens: u32,
    position: u32,
    d_k: u32,
    d_v: u32,
    pages_per_row: u32,
    depth: u8,
    root: Hash64,
    parameters: [u8; 68],
    phase: PhaseV1,
    row: u32,
    page: u32,
    accumulator: i64,
    delta: i32,
    decay: i64,
    beta: i64,
    key: Vec<i32>,
    value: Vec<i32>,
}
fn h(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut s = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for p in parts {
        s.update(p);
    }
    Hash64::from_bytes(s.finalize().as_bytes().try_into().expect("full512 hash"))
}
pub fn palw_gdn_owned_page_hash_v1(values: &[i32]) -> Result<Hash64, String> {
    if values.len() != PALW_GDN_PAGE_WORDS_V1 {
        return Err("owned page has noncanonical lane count".into());
    }
    let mut s = blake2b_simd::Params::new().hash_length(64).key(LEAF).to_state();
    for v in values {
        s.update(&v.to_le_bytes());
    }
    Ok(Hash64::from_bytes(s.finalize().as_bytes().try_into().expect("full512 hash")))
}
pub fn palw_gdn_owned_node_v1(a: &Hash64, b: &Hash64) -> Hash64 {
    h(NODE, &[a.as_byte_slice(), b.as_byte_slice()])
}
impl PalwGdnPagedStateV1 {
    /// Internal initial state only. Context belongs to the authenticated outer claim/seed/head;
    /// registered parameters and public inputs are authenticated there, never by this codec.
    pub(crate) fn new(context: Hash64, prefix_tokens: u32, d_k: u32, d_v: u32, parameters: [u8; 68]) -> Result<Self, String> {
        if prefix_tokens == 0 || d_k == 0 || d_v == 0 || d_k > 1 << 17 || d_v > 1 << 17 {
            return Err("paged recurrence geometry exceeds the existing kernel domain".into());
        }
        let p = Self::decode_parameters(&parameters)?;
        for m in [p.0.multiplier, p.1.multiplier, p.3.multiplier] {
            if m.unsigned_abs() > QWEN36_STATE_MULT_MAX.unsigned_abs() {
                return Err("registered state multiplier outside kernel domain".into());
            }
        }
        if !(-62..=20).contains(&(p.2.zero as i32)) {
            return Err("registered state write shift outside kernel domain".into());
        }
        let pages_per_row = d_k.div_ceil(PALW_GDN_PAGE_WORDS_V1 as u32);
        let pages = u64::from(pages_per_row) * u64::from(d_v);
        let padded = pages.next_power_of_two();
        let depth = padded.trailing_zeros() as u8;
        let zeros = [0i32; PALW_GDN_PAGE_WORDS_V1];
        let mut root = palw_gdn_owned_page_hash_v1(&zeros)?;
        for _ in 0..depth {
            root = palw_gdn_owned_node_v1(&root, &root);
        }
        Ok(Self {
            context,
            prefix_tokens,
            position: 0,
            d_k,
            d_v,
            pages_per_row,
            depth,
            root,
            parameters,
            phase: PhaseV1::Idle,
            row: 0,
            page: 0,
            accumulator: 0,
            delta: 0,
            decay: 0,
            beta: 0,
            key: vec![],
            value: vec![],
        })
    }
    fn decode_parameters(p: &[u8; 68]) -> Result<(A16QuantParams, A16QuantParams, A16QuantParams, A16QuantParams), String> {
        let at =
            |i: usize| A16QuantParams::from_wire(&p[i * 17..(i + 1) * 17]).map_err(|e| format!("registered parameter domain: {e:?}"));
        Ok((at(0)?, at(1)?, at(2)?, at(3)?))
    }
    pub fn root(&self) -> Hash64 {
        self.root
    }
    pub fn position(&self) -> u32 {
        self.position
    }
    pub fn page_count(&self) -> u64 {
        u64::from(self.pages_per_row) * u64::from(self.d_v)
    }
    pub fn depth(&self) -> u8 {
        self.depth
    }
    pub fn next_page(&self) -> Option<(u64, bool)> {
        (self.phase != PhaseV1::Idle)
            .then(|| (u64::from(self.row) * u64::from(self.pages_per_row) + u64::from(self.page), self.phase == PhaseV1::Write))
    }
    pub fn complete(&self) -> bool {
        self.position == self.prefix_tokens && self.phase == PhaseV1::Idle
    }
    pub fn check(&self) -> Result<(), String> {
        if self.position > self.prefix_tokens
            || self.row >= self.d_v
            || self.page >= self.pages_per_row
            || (self.phase == PhaseV1::Idle && (!self.key.is_empty() || !self.value.is_empty()))
            || (self.phase != PhaseV1::Idle && (self.key.len() != self.d_k as usize || self.value.len() != self.d_v as usize))
        {
            return Err("paged recurrence counters/input lengths inconsistent".into());
        }
        Ok(())
    }
    pub(crate) fn begin_position(
        &mut self,
        context: Hash64,
        key: &[i32],
        value: &[i32],
        query: &[i32],
        decay: i64,
        beta: i64,
    ) -> Result<(), String> {
        self.check()?;
        if context != self.context
            || self.phase != PhaseV1::Idle
            || self.position >= self.prefix_tokens
            || key.len() != self.d_k as usize
            || query.len() != self.d_k as usize
            || value.len() != self.d_v as usize
            || key.iter().chain(value).chain(query).any(|v| !(-A16_CODE_MAX..=A16_CODE_MAX).contains(&i64::from(*v)))
            || !(0..=ONE).contains(&decay)
            || !(0..=ONE).contains(&beta)
        {
            return Err("owned paged position changes bounded authenticated kernel inputs".into());
        }
        self.key = key.to_vec();
        self.value = value.to_vec();
        self.decay = decay;
        self.beta = beta;
        self.row = 0;
        self.page = 0;
        self.accumulator = 0;
        self.delta = 0;
        self.phase = PhaseV1::Read;
        Ok(())
    }
    fn opening(&self, page: &PalwGdnOwnedPageOpeningV1) -> Result<(u64, usize), String> {
        let (index, _) = self.next_page().ok_or("paged recurrence has no current page")?;
        Ok((index, self.authenticate_page(index, page)?))
    }
    fn authenticate_page(&self, index: u64, page: &PalwGdnOwnedPageOpeningV1) -> Result<usize, String> {
        if index >= self.page_count() {
            return Err("owned page index outside exact head geometry".into());
        }
        if page.siblings.len() != self.depth as usize {
            return Err("owned page path has noncanonical exact depth".into());
        }
        let mut root = palw_gdn_owned_page_hash_v1(&page.values)?;
        let mut at = index;
        for sibling in &page.siblings {
            root = if at & 1 == 0 { palw_gdn_owned_node_v1(&root, sibling) } else { palw_gdn_owned_node_v1(sibling, &root) };
            at >>= 1;
        }
        if root != self.root {
            return Err("page does not authenticate against owned recurrence root".into());
        }
        let start = (index % u64::from(self.pages_per_row)) as usize * PALW_GDN_PAGE_WORDS_V1;
        let count = (self.d_k as usize - start).min(PALW_GDN_PAGE_WORDS_V1);
        if page.values[count..].iter().any(|v| *v != 0) {
            return Err("owned page padding is not canonical zero".into());
        }
        Ok(count)
    }
    pub(crate) fn completed_page<'a>(
        &self,
        context: Hash64,
        index: u64,
        page: &'a PalwGdnOwnedPageOpeningV1,
    ) -> Result<&'a [i32], String> {
        self.check()?;
        if context != self.context || !self.complete() {
            return Err("owned matrix is not a completed authenticated prefix".into());
        }
        let count = self.authenticate_page(index, page)?;
        Ok(&page.values[..count])
    }
    pub(crate) fn read_page(&mut self, context: Hash64, page: &PalwGdnOwnedPageOpeningV1) -> Result<(), String> {
        self.check()?;
        if context != self.context || self.phase != PhaseV1::Read {
            return Err("owned page is not in its read phase".into());
        }
        let (_, count) = self.opening(page)?;
        let start = self.page as usize * PALW_GDN_PAGE_WORDS_V1;
        let mut acc = self.accumulator;
        for (s, k) in page.values[..count].iter().zip(&self.key[start..start + count]) {
            let gated = rounding_shift_right_64(i64::from(*s) * self.decay, K as u8).clamp(-QWEN36_STATE_MAX, QWEN36_STATE_MAX);
            acc = acc.checked_add(gated * i64::from(*k)).ok_or("owned exact state/key accumulator overflow")?;
        }
        if self.page + 1 == self.pages_per_row {
            let (read, delta, _, _) = Self::decode_parameters(&self.parameters)?;
            let w = a16_scale_round(acc, read.multiplier, read.shift).saturating_add(read.zero);
            let d = i64::from(self.value[self.row as usize]).saturating_sub(w);
            let scaled = rounding_shift_right_64(d.saturating_mul(self.beta), K as u8);
            let u = a16_scale_round(scaled, delta.multiplier, delta.shift).saturating_add(delta.zero);
            self.delta = u.clamp(-QWEN36_DELTA_MAX, QWEN36_DELTA_MAX) as i32;
            self.accumulator = acc;
            self.page = 0;
            self.phase = PhaseV1::Write;
        } else {
            self.accumulator = acc;
            self.page += 1;
        }
        Ok(())
    }
    /// Returns the replacement page for public storage/proof generation. Only these computed
    /// values can advance the rooted state; no prover-supplied new page/hash is accepted.
    pub(crate) fn write_page(&mut self, context: Hash64, page: &PalwGdnOwnedPageOpeningV1) -> Result<Vec<i32>, String> {
        self.check()?;
        if context != self.context || self.phase != PhaseV1::Write {
            return Err("owned page is not in its write phase".into());
        }
        let (index, count) = self.opening(page)?;
        let (_, _, write, _) = Self::decode_parameters(&self.parameters)?;
        let shift = write.zero as i32;
        let start = self.page as usize * PALW_GDN_PAGE_WORDS_V1;
        let mut values = vec![0; PALW_GDN_PAGE_WORDS_V1];
        for (i, (s, k)) in page.values[..count].iter().zip(&self.key[start..start + count]).enumerate() {
            let gated = rounding_shift_right_64(i64::from(*s) * self.decay, K as u8).clamp(-QWEN36_STATE_MAX, QWEN36_STATE_MAX);
            let product = i64::from(self.delta) * i64::from(*k);
            let term =
                if shift >= 0 { product.saturating_mul(1i64 << shift) } else { rounding_shift_right_64(product, (-shift) as u8) };
            values[i] = gated.saturating_add(term).clamp(-QWEN36_STATE_MAX, QWEN36_STATE_MAX) as i32;
        }
        let mut root = palw_gdn_owned_page_hash_v1(&values)?;
        let mut at = index;
        for sibling in &page.siblings {
            root = if at & 1 == 0 { palw_gdn_owned_node_v1(&root, sibling) } else { palw_gdn_owned_node_v1(sibling, &root) };
            at >>= 1;
        }
        self.root = root;
        if self.page + 1 == self.pages_per_row {
            self.page = 0;
            self.accumulator = 0;
            self.delta = 0;
            if self.row + 1 == self.d_v {
                self.position += 1;
                self.row = 0;
                self.phase = PhaseV1::Idle;
                self.key.clear();
                self.value.clear();
            } else {
                self.row += 1;
                self.phase = PhaseV1::Read;
            }
        } else {
            self.page += 1;
        }
        Ok(values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_qwen36_ops::{Qwen36GdnParamsV1, Qwen36GdnStateV1, q36_gdn_step};
    struct Book {
        pages: Vec<Vec<i32>>,
    }
    impl Book {
        fn new(count: u64) -> Self {
            Self { pages: vec![vec![0; PALW_GDN_PAGE_WORDS_V1]; count.next_power_of_two() as usize] }
        }
        fn levels(&self) -> Vec<Vec<Hash64>> {
            let mut out = vec![self.pages.iter().map(|p| palw_gdn_owned_page_hash_v1(p).unwrap()).collect::<Vec<_>>()];
            while out.last().unwrap().len() > 1 {
                out.push(out.last().unwrap().chunks_exact(2).map(|p| palw_gdn_owned_node_v1(&p[0], &p[1])).collect());
            }
            out
        }
        fn root(&self) -> Hash64 {
            *self.levels().last().unwrap().first().unwrap()
        }
        fn open(&self, index: u64) -> PalwGdnOwnedPageOpeningV1 {
            let mut at = index as usize;
            let mut siblings = vec![];
            for level in self.levels().iter().take(self.levels().len() - 1) {
                siblings.push(level[at ^ 1]);
                at /= 2;
            }
            PalwGdnOwnedPageOpeningV1 { values: self.pages[index as usize].clone(), siblings }
        }
    }
    fn params(shift: i32) -> ([u8; 68], Qwen36GdnParamsV1) {
        let read = A16QuantParams { multiplier: 1, shift: 15, zero: -7 };
        let delta = A16QuantParams { multiplier: 3, shift: 2, zero: 11 };
        let write = A16QuantParams { multiplier: 0, shift: 0, zero: i64::from(shift) };
        let out = A16QuantParams { multiplier: 1, shift: 15, zero: 0 };
        let mut bytes = [0; 68];
        for (i, p) in [read, delta, write, out].iter().enumerate() {
            bytes[i * 17..(i + 1) * 17].copy_from_slice(&p.to_wire());
        }
        (bytes, Qwen36GdnParamsV1 { read, delta, write_shift: shift, out })
    }
    #[test]
    fn paged_owned_matrix_matches_exact_native_recurrence_across_partial_pages_and_restart() {
        for (dk, dv) in [(1, 1), (4097, 2)] {
            for shift in [-3, 0, 2] {
                let context = Hash64::from_u64_word(0x1719);
                let (wire, p) = params(shift);
                let mut court = PalwGdnPagedStateV1::new(context, 3, dk, dv, wire).unwrap();
                let mut book = Book::new(court.page_count());
                assert_eq!(court.root(), book.root());
                let mut native = Qwen36GdnStateV1::zeros(dv as usize, dk as usize);
                for position in 0..3 {
                    let key: Vec<i32> = (0..dk).map(|i| (i as i32 % 127) - 63).collect();
                    let value: Vec<i32> = (0..dv).map(|i| (i as i32 * 11) - 19 - position).collect();
                    let query = key.clone();
                    let decay = ONE - (ONE / 7) * i64::from(position);
                    let beta = ONE / 3;
                    q36_gdn_step(&mut native, &key, &value, &query, decay, beta, p).unwrap();
                    court.begin_position(context, &key, &value, &query, decay, beta).unwrap();
                    while let Some((index, write)) = court.next_page() {
                        let opening = book.open(index);
                        let before = borsh::to_vec(&court).unwrap();
                        let mut forged = opening.clone();
                        forged.values[0] ^= 1;
                        if write {
                            assert!(court.write_page(context, &forged).is_err());
                        } else {
                            assert!(court.read_page(context, &forged).is_err());
                        }
                        assert_eq!(borsh::to_vec(&court).unwrap(), before);
                        if write {
                            book.pages[index as usize] = court.write_page(context, &opening).unwrap();
                        } else {
                            court.read_page(context, &opening).unwrap();
                        }
                        court = borsh::from_slice(&borsh::to_vec(&court).unwrap()).unwrap();
                        assert_eq!(court.root(), book.root());
                    }
                    let actual: Vec<_> = (0..court.page_count())
                        .flat_map(|i| {
                            let row_page = (i % u64::from(court.pages_per_row)) as usize;
                            let count = (dk as usize - row_page * PALW_GDN_PAGE_WORDS_V1).min(PALW_GDN_PAGE_WORDS_V1);
                            book.pages[i as usize][..count].iter().copied()
                        })
                        .collect();
                    assert_eq!(actual, native.s, "exact rounding/decay/read/delta/write, dk={dk}, shift={shift}");
                }
                assert!(court.complete());
                let mut completed = vec![];
                for index in 0..court.page_count() {
                    completed.extend_from_slice(court.completed_page(context, index, &book.open(index)).unwrap());
                }
                assert_eq!(completed, native.s);
            }
        }
    }
    #[test]
    fn maximum_kernel_geometry_initializes_constant_resident_zero_root_without_a_matrix_allocation() {
        let (wire, _) = params(0);
        let c = PalwGdnPagedStateV1::new(Hash64::from_u64_word(1), 1, 1 << 17, 1 << 17, wire).unwrap();
        assert_eq!(c.page_count(), 1 << 22);
        assert_eq!(c.depth(), 22);
        assert!(borsh::to_vec(&c).unwrap().len() < 512);
        assert!(PalwGdnPagedStateV1::new(Hash64::default(), 1, (1 << 17) + 1, 1, wire).is_err());
    }
}
