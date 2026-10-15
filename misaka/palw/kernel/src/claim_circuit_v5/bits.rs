//! Little-endian Boolean integer operations. Every carry and selector is an
//! explicit gate; a host integer result is never substituted for witness work.
use super::*;
impl CircuitBuilderV5 {
    pub fn constant_word<const N: usize>(&self, value: u128) -> [WireV5; N] {
        std::array::from_fn(|i| self.constant(i < 128 && (value >> i) & 1 != 0))
    }
    pub fn xor_word<const N: usize>(&mut self, a: &[WireV5; N], b: &[WireV5; N]) -> Result<[WireV5; N], String> {
        let mut out = [0; N];
        for i in 0..N {
            out[i] = self.xor(a[i], b[i])?;
        }
        Ok(out)
    }
    /// Wrapping addition (the full-adder carry has five Boolean gates per bit).
    pub fn add_word<const N: usize>(&mut self, a: &[WireV5; N], b: &[WireV5; N]) -> Result<([WireV5; N], WireV5), String> {
        let mut carry = self.constant(false);
        let mut out = [0; N];
        for i in 0..N {
            let x = self.xor(a[i], b[i])?;
            out[i] = self.xor(x, carry)?;
            let ab = self.and(a[i], b[i])?;
            let xc = self.and(x, carry)?;
            carry = self.xor(ab, xc)?;
        }
        Ok((out, carry))
    }
    pub fn select_word<const N: usize>(&mut self, condition: WireV5, a: &[WireV5; N], b: &[WireV5; N]) -> Result<[WireV5; N], String> {
        let mut out = [0; N];
        for i in 0..N {
            let difference = self.xor(a[i], b[i])?;
            let selected = self.and(condition, difference)?;
            out[i] = self.xor(b[i], selected)?;
        }
        Ok(out)
    }
    pub fn not_word<const N: usize>(&mut self, a: &[WireV5; N]) -> Result<[WireV5; N], String> {
        let mut out = [0; N];
        for i in 0..N {
            out[i] = self.not(a[i])?;
        }
        Ok(out)
    }
    pub fn negate_word<const N: usize>(&mut self, a: &[WireV5; N]) -> Result<[WireV5; N], String> {
        let inverted = self.not_word(a)?;
        let one = self.constant_word::<N>(1);
        Ok(self.add_word(&inverted, &one)?.0)
    }
    pub fn subtract_word<const N: usize>(&mut self, a: &[WireV5; N], b: &[WireV5; N]) -> Result<[WireV5; N], String> {
        let negative = self.negate_word(b)?;
        Ok(self.add_word(a, &negative)?.0)
    }
    pub fn unsigned_less<const N: usize>(&mut self, a: &[WireV5; N], b: &[WireV5; N]) -> Result<WireV5, String> {
        let mut less = self.constant(false);
        for i in 0..N {
            let na = self.not(a[i])?;
            let ab = self.and(na, b[i])?;
            let different = self.xor(a[i], b[i])?;
            let equal = self.not(different)?;
            let previous = self.and(equal, less)?;
            less = self.xor(ab, previous)?;
        }
        Ok(less)
    }
    pub fn signed_less<const N: usize>(&mut self, a: &[WireV5; N], b: &[WireV5; N]) -> Result<WireV5, String> {
        if N == 0 {
            return Err("zero-width signed integer".into());
        }
        let mut aa = *a;
        let mut bb = *b;
        aa[N - 1] = self.not(aa[N - 1])?;
        bb[N - 1] = self.not(bb[N - 1])?;
        self.unsigned_less(&aa, &bb)
    }
    pub fn equal_word<const N: usize>(&mut self, a: &[WireV5; N], b: &[WireV5; N]) -> Result<WireV5, String> {
        let mut equal = self.constant(true);
        for i in 0..N {
            let different = self.xor(a[i], b[i])?;
            let same = self.not(different)?;
            equal = self.and(equal, same)?;
        }
        Ok(equal)
    }
    /// Schoolbook product modulo 2^N. Wider exact products explicitly instantiate
    /// a wider N and assert the required high/sign-extension bits separately.
    pub fn multiply_word<const N: usize>(&mut self, a: &[WireV5; N], b: &[WireV5; N]) -> Result<[WireV5; N], String> {
        let mut out = self.constant_word::<N>(0);
        for i in 0..N {
            let mut row = self.constant_word::<N>(0);
            for j in 0..N - i {
                row[i + j] = self.and(a[j], b[i])?;
            }
            out = self.add_word(&out, &row)?.0;
        }
        Ok(out)
    }
}
