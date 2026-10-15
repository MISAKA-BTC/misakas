//! Exact i128 scalar semantics emitted as Boolean gates. The three rounding
//! rules and frozen Q24 algorithms are transcribed from tir/core/arith.rs;
//! branches use selectors, and every fixed iteration is explicit circuit work.
use super::*;
use misaka_palw_tir::{DType, prim::Rounding};
pub type IntV5 = [WireV5; 128];
impl CircuitBuilderV5 {
    pub fn integer(&self, value: i128) -> IntV5 {
        self.constant_word(value as u128)
    }
    pub fn or(&mut self, a: WireV5, b: WireV5) -> Result<WireV5, String> {
        let x = self.xor(a, b)?;
        let both = self.and(a, b)?;
        self.xor(x, both)
    }
    pub fn nonzero(&mut self, value: &IntV5) -> Result<WireV5, String> {
        let mut any = self.constant(false);
        for bit in value {
            any = self.or(any, *bit)?;
        }
        Ok(any)
    }
    pub fn positive(&mut self, value: &IntV5) -> Result<WireV5, String> {
        let zero = self.integer(0);
        self.signed_less(&zero, value)
    }
    pub fn assert_integer_equal(&mut self, a: &IntV5, b: &IntV5) -> Result<(), String> {
        let same = self.equal_word(a, b)?;
        self.assert(same, true)
    }
    pub fn assert_fit(&mut self, value: &IntV5, dtype: DType) -> Result<(), String> {
        let width = dtype.width() * 8;
        let fill = if dtype == DType::Idx { self.constant(false) } else { value[width - 1] };
        for bit in &value[width..] {
            let difference = self.xor(*bit, fill)?;
            self.assert(difference, false)?;
        }
        Ok(())
    }
    pub fn assert_interval(&mut self, value: &IntV5, lo: i128, hi: i128) -> Result<(), String> {
        if lo > hi {
            return Err("reversed scalar interval".into());
        }
        let lower = self.integer(lo);
        let upper = self.integer(hi);
        let below = self.signed_less(value, &lower)?;
        let above = self.signed_less(&upper, value)?;
        self.assert(below, false)?;
        self.assert(above, false)
    }
    pub fn read_integer(&self, data: &dyn CircuitByteStreamV5, element: usize, dtype: DType) -> Result<IntV5, String> {
        let width = dtype.width();
        let at = element.checked_mul(width).ok_or("scalar input offset overflow")?;
        let mut value = [0; 128];
        for i in 0..width {
            let byte = data.byte(at.checked_add(i).ok_or("scalar input byte overflow")?)?;
            value[i * 8..i * 8 + 8].copy_from_slice(&byte);
        }
        let fill = if dtype == DType::Idx { self.constant(false) } else { value[width * 8 - 1] };
        value[width * 8..].fill(fill);
        Ok(value)
    }
    pub fn checked_add_integer(&mut self, a: &IntV5, b: &IntV5) -> Result<IntV5, String> {
        let result = self.add_word(a, b)?.0;
        let unequal = self.xor(a[127], b[127])?;
        let same = self.not(unequal)?;
        let changed = self.xor(result[127], a[127])?;
        let overflow = self.and(same, changed)?;
        self.assert(overflow, false)?;
        Ok(result)
    }
    pub fn checked_subtract_integer(&mut self, a: &IntV5, b: &IntV5) -> Result<IntV5, String> {
        let result = self.subtract_word(a, b)?;
        let different = self.xor(a[127], b[127])?;
        let changed = self.xor(result[127], a[127])?;
        let overflow = self.and(different, changed)?;
        self.assert(overflow, false)?;
        Ok(result)
    }
    pub fn checked_multiply_integer(&mut self, a: &IntV5, b: &IntV5) -> Result<IntV5, String> {
        let wide_a: [WireV5; 256] = std::array::from_fn(|i| if i < 128 { a[i] } else { a[127] });
        let wide_b: [WireV5; 256] = std::array::from_fn(|i| if i < 128 { b[i] } else { b[127] });
        let wide = self.multiply_word(&wide_a, &wide_b)?;
        let result: IntV5 = std::array::from_fn(|i| wide[i]);
        for bit in &wide[128..] {
            let difference = self.xor(*bit, result[127])?;
            self.assert(difference, false)?;
        }
        Ok(result)
    }
    pub fn clamp_integer(&mut self, value: &IntV5, lo: i128, hi: i128) -> Result<IntV5, String> {
        if lo > hi {
            return Err("reversed scalar clamp".into());
        }
        let lower = self.integer(lo);
        let upper = self.integer(hi);
        let below = self.signed_less(value, &lower)?;
        let low = self.select_word(below, &lower, value)?;
        let above = self.signed_less(&upper, &low)?;
        self.select_word(above, &upper, &low)
    }
    /// Restoring unsigned division with a 129-bit remainder. The extra bit is
    /// necessary when a high remainder is shifted; truncating it is unsound.
    pub fn unsigned_divide_integer(&mut self, numerator: &IntV5, denominator: &IntV5) -> Result<(IntV5, IntV5), String> {
        let nonzero = self.nonzero(denominator)?;
        self.assert(nonzero, true)?;
        let divisor: [WireV5; 129] = std::array::from_fn(|i| if i < 128 { denominator[i] } else { self.constant(false) });
        let mut remainder = self.constant_word::<129>(0);
        let mut quotient = self.integer(0);
        for index in (0..128).rev() {
            for i in (1..129).rev() {
                remainder[i] = remainder[i - 1];
            }
            remainder[0] = numerator[index];
            let less = self.unsigned_less(&remainder, &divisor)?;
            let ge = self.not(less)?;
            let difference = self.subtract_word(&remainder, &divisor)?;
            remainder = self.select_word(ge, &difference, &remainder)?;
            quotient[index] = ge;
        }
        self.assert(remainder[128], false)?;
        Ok((quotient, std::array::from_fn(|i| remainder[i])))
    }
    pub fn divide_round_integer(&mut self, x: &IntV5, d: &IntV5, rule: Rounding) -> Result<IntV5, String> {
        let positive = self.positive(d)?;
        self.assert(positive, true)?;
        let negative = x[127];
        let negated = self.negate_word(x)?;
        let magnitude = self.select_word(negative, &negated, x)?;
        let (q, r) = self.unsigned_divide_integer(&magnitude, d)?;
        let one = self.integer(1);
        let zero = self.integer(0);
        match rule {
            Rounding::HalfAwayFromZero => {
                let distance = self.subtract_word(d, &r)?;
                let below = self.unsigned_less(&r, &distance)?;
                let round = self.not(below)?;
                let rounded = self.add_word(&q, &one)?.0;
                let rounded = self.select_word(round, &rounded, &q)?;
                let negated = self.negate_word(&rounded)?;
                self.select_word(negative, &negated, &rounded)
            }
            Rounding::Floor | Rounding::HalfUp => {
                let remainder_nonzero = self.nonzero(&r)?;
                let borrowed = self.and(negative, remainder_nonzero)?;
                let negative_q = self.negate_word(&q)?;
                let lowered = self.subtract_word(&negative_q, &one)?;
                let negative_q = self.select_word(borrowed, &lowered, &negative_q)?;
                let floor = self.select_word(negative, &negative_q, &q)?;
                if rule == Rounding::Floor {
                    return Ok(floor);
                }
                let complemented = self.subtract_word(d, &r)?;
                let euclidean = self.select_word(borrowed, &complemented, &r)?;
                let distance = self.subtract_word(d, &euclidean)?;
                let below = self.unsigned_less(&euclidean, &distance)?;
                let round = self.not(below)?;
                let increment = self.select_word(round, &one, &zero)?;
                Ok(self.add_word(&floor, &increment)?.0)
            }
        }
    }
    pub fn shift_integer(&mut self, value: &IntV5, amount: &IntV5, left: bool) -> Result<IntV5, String> {
        let mut result = *value;
        let fill = if left { self.constant(false) } else { value[127] };
        for (power, control) in amount[..7].iter().enumerate() {
            let offset = 1usize << power;
            let shifted: IntV5 = std::array::from_fn(|i| {
                if left {
                    if i >= offset { result[i - offset] } else { fill }
                } else {
                    result.get(i + offset).copied().unwrap_or(fill)
                }
            });
            result = self.select_word(*control, &shifted, &result)?;
        }
        let mut large = self.constant(false);
        for bit in &amount[7..] {
            large = self.or(large, *bit)?;
        }
        self.select_word(large, &[fill; 128], &result)
    }
    pub fn shift_constant_integer(&self, value: &IntV5, amount: usize, left: bool) -> IntV5 {
        let fill = if left { self.constant(false) } else { value[127] };
        std::array::from_fn(|i| {
            if left {
                if i >= amount { value[i - amount] } else { fill }
            } else {
                value.get(i.saturating_add(amount)).copied().unwrap_or(fill)
            }
        })
    }
    pub fn normalize_integer(&mut self, value: &IntV5, signed_right_shift: &IntV5) -> Result<IntV5, String> {
        let negative = signed_right_shift[127];
        let negate = self.negate_word(signed_right_shift)?;
        let magnitude = self.select_word(negative, &negate, signed_right_shift)?;
        let left = self.shift_integer(value, &magnitude, true)?;
        let right = self.shift_integer(value, &magnitude, false)?;
        self.select_word(negative, &left, &right)
    }
    pub fn log2_floor_integer(&mut self, x: &IntV5) -> Result<IntV5, String> {
        let mut result = self.integer(-1);
        for (i, bit) in x.iter().enumerate() {
            let index = self.integer(i as i128);
            result = self.select_word(*bit, &index, &result)?;
        }
        let positive = self.positive(x)?;
        let minus_one = self.integer(-1);
        self.select_word(positive, &result, &minus_one)
    }
    pub fn int_exp_integer(&mut self, input: &IntV5) -> Result<IntV5, String> {
        use misaka_palw_tir::arith::*;
        let x = self.clamp_integer(input, i128::MIN, 0)?;
        let threshold = self.integer(EXP_ZERO_AT);
        let active = self.signed_less(&threshold, &x)?;
        let zero = self.integer(0);
        let x = self.select_word(active, &x, &zero)?;
        let negative = self.negate_word(&x)?;
        let ln2 = self.integer(LN2_Q);
        let (z, _) = self.unsigned_divide_integer(&negative, &ln2)?;
        let zln = self.checked_multiply_integer(&z, &ln2)?;
        let p = self.checked_add_integer(&x, &zln)?;
        let b = self.integer(POLY2_B);
        let t = self.checked_add_integer(&p, &b)?;
        let square = self.checked_multiply_integer(&t, &t)?;
        let square = self.shift_constant_integer(&square, K as usize, false);
        let a = self.integer(POLY2_A);
        let poly = self.checked_multiply_integer(&a, &square)?;
        let poly = self.shift_constant_integer(&poly, K as usize, false);
        let c = self.integer(POLY2_C);
        let poly = self.checked_add_integer(&poly, &c)?;
        let one = self.integer(1);
        let divisor = self.shift_integer(&one, &z, true)?;
        let result = self.divide_round_integer(&poly, &divisor, Rounding::HalfAwayFromZero)?;
        self.select_word(active, &result, &zero)
    }
    pub fn int_rsqrt_integer(&mut self, input: &IntV5) -> Result<IntV5, String> {
        use misaka_palw_tir::arith::*;
        let positive = self.positive(input)?;
        let one = self.integer(1);
        let v = self.select_word(positive, input, &one)?;
        let bit = self.log2_floor_integer(&v)?;
        let k = self.integer(K as i128);
        let shift = self.checked_subtract_integer(&bit, &k)?;
        let e = self.shift_constant_integer(&shift, 1, false);
        let twice = self.shift_constant_integer(&e, 1, true);
        let m = self.normalize_integer(&v, &twice)?;
        // For positive v, e=floor((floor(log2 v)-24)/2) proves
        // 2^24 <= m < 2^26. The reference normalization loops therefore run
        // zero times; assert that invariant rather than omitting it silently.
        self.assert_interval(&m, ONE, 4 * ONE - 1)?;
        let unit = self.integer(ONE);
        let difference = self.checked_subtract_integer(&m, &unit)?;
        let numerator = self.shift_constant_integer(&difference, 4, true);
        let denominator = self.integer(3 * ONE);
        let (index, _) = self.unsigned_divide_integer(&numerator, &denominator)?;
        let index = self.clamp_integer(&index, 0, 15)?;
        let mut y = self.integer(RSQRT_SEED[0]);
        for (i, seed) in RSQRT_SEED.iter().enumerate().skip(1) {
            let selected = self.equal_word(&index, &self.integer(i as i128))?;
            y = self.select_word(selected, &self.integer(*seed), &y)?;
        }
        for _ in 0..RSQRT_ITERS {
            let y2 = self.checked_multiply_integer(&y, &y)?;
            let y2 = self.shift_constant_integer(&y2, K as usize, false);
            let my2 = self.checked_multiply_integer(&m, &y2)?;
            let my2 = self.shift_constant_integer(&my2, K as usize, false);
            let three = self.integer(3 * ONE);
            let difference = self.checked_subtract_integer(&three, &my2)?;
            let product = self.checked_multiply_integer(&y, &difference)?;
            let next = self.shift_constant_integer(&product, (K + 1) as usize, false);
            let next_positive = self.positive(&next)?;
            y = self.select_word(next_positive, &next, &one)?;
        }
        let result = self.normalize_integer(&y, &e)?;
        let zero = self.integer(0);
        self.select_word(positive, &result, &zero)
    }
    pub fn int_ln_integer(&mut self, input: &IntV5) -> Result<IntV5, String> {
        use misaka_palw_tir::arith::*;
        let positive = self.positive(input)?;
        let one = self.integer(1);
        let x = self.select_word(positive, input, &one)?;
        let log = self.log2_floor_integer(&x)?;
        let k = self.integer(K as i128);
        let s = self.checked_subtract_integer(&log, &k)?;
        let m = self.normalize_integer(&x, &s)?;
        self.assert_interval(&m, ONE, 2 * ONE - 1)?;
        let unit = self.integer(ONE);
        let sub = self.checked_subtract_integer(&m, &unit)?;
        let numerator = self.shift_constant_integer(&sub, K as usize, true);
        let denominator = self.checked_add_integer(&m, &unit)?;
        let (t, _) = self.unsigned_divide_integer(&numerator, &denominator)?;
        let t2 = self.checked_multiply_integer(&t, &t)?;
        let t2 = self.shift_constant_integer(&t2, K as usize, false);
        let mut term = t;
        let mut sum = t;
        for odd in [3i128, 5, 7, 9, 11] {
            term = self.checked_multiply_integer(&term, &t2)?;
            term = self.shift_constant_integer(&term, K as usize, false);
            let divisor = self.integer(odd);
            let (quotient, _) = self.unsigned_divide_integer(&term, &divisor)?;
            sum = self.checked_add_integer(&sum, &quotient)?;
        }
        let twice = self.shift_constant_integer(&sum, 1, true);
        let ln2 = self.integer(LN2_Q);
        let scale = self.checked_multiply_integer(&s, &ln2)?;
        let result = self.checked_add_integer(&twice, &scale)?;
        let zero = self.integer(0);
        self.select_word(positive, &result, &zero)
    }
}

pub(crate) fn emit_scalar_v5(
    builder: &mut CircuitBuilderV5,
    prim: &misaka_palw_tir::Prim,
    inputs: &[IntV5],
    output: DType,
) -> Result<IntV5, String> {
    use misaka_palw_tir::{Prim, prim::Cmp};
    let result = match prim {
        Prim::Cast => *inputs.first().ok_or("cast operand missing")?,
        Prim::Add => builder.checked_add_integer(&inputs[0], &inputs[1])?,
        Prim::Sub => builder.checked_subtract_integer(&inputs[0], &inputs[1])?,
        Prim::Mul => builder.checked_multiply_integer(&inputs[0], &inputs[1])?,
        Prim::Div { rule } => builder.divide_round_integer(&inputs[0], &inputs[1], *rule)?,
        Prim::Clamp { lo, hi } => builder.clamp_integer(&inputs[0], i128::from(*lo), i128::from(*hi))?,
        Prim::Log2Floor => builder.log2_floor_integer(&inputs[0])?,
        Prim::IntExp => builder.int_exp_integer(&inputs[0])?,
        Prim::IntRsqrt => builder.int_rsqrt_integer(&inputs[0])?,
        Prim::IntLn => builder.int_ln_integer(&inputs[0])?,
        Prim::Compare { cmp } => {
            let bit = match cmp {
                Cmp::Eq => builder.equal_word(&inputs[0], &inputs[1])?,
                Cmp::Ne => {
                    let equal = builder.equal_word(&inputs[0], &inputs[1])?;
                    builder.not(equal)?
                }
                Cmp::Lt => builder.signed_less(&inputs[0], &inputs[1])?,
                Cmp::Le => {
                    let gt = builder.signed_less(&inputs[1], &inputs[0])?;
                    builder.not(gt)?
                }
                Cmp::Gt => builder.signed_less(&inputs[1], &inputs[0])?,
                Cmp::Ge => {
                    let lt = builder.signed_less(&inputs[0], &inputs[1])?;
                    builder.not(lt)?
                }
            };
            let mut out = builder.integer(0);
            out[0] = bit;
            out
        }
        Prim::Select => {
            let condition = builder.nonzero(&inputs[0])?;
            builder.select_word(condition, &inputs[1], &inputs[2])?
        }
        _ => return Err("this scalar entry requires tensor indexing or state context".into()),
    };
    builder.assert_fit(&result, output)?;
    Ok(result)
}
fn scalar_arity(prim: &misaka_palw_tir::Prim) -> Result<usize, String> {
    use misaka_palw_tir::Prim;
    match prim {
        Prim::Cast | Prim::Clamp { .. } | Prim::Log2Floor | Prim::IntExp | Prim::IntRsqrt | Prim::IntLn => Ok(1),
        Prim::Add | Prim::Sub | Prim::Mul | Prim::Div { .. } | Prim::Compare { .. } => Ok(2),
        Prim::Select => Ok(3),
        _ => Err("not a stateless scalar primitive".into()),
    }
}
/// Independent exact scalar relation compiler. Tensor lowering reuses the same
/// gate routines with native wiring, dimensions, interval and state authority.
pub fn compile_scalar_v5(
    prim: &misaka_palw_tir::Prim,
    input_types: &[DType],
    output: DType,
    expected: i128,
    limits: CircuitLimitsV5,
    memory: CircuitMemoryBudgetV5,
) -> Result<ReservedCircuitV5, String> {
    if input_types.len() != scalar_arity(prim)? || !output.contains(expected) {
        return Err("scalar arity/expected output dtype mismatch".into());
    }
    let types: [misaka_palw_tir::TensorType; 3] =
        std::array::from_fn(|i| misaka_palw_tir::TensorType::scalar(input_types.get(i).copied().unwrap_or(DType::I8)));
    misaka_palw_tir::validate::check_node_type(prim, &types[..input_types.len()], &misaka_palw_tir::TensorType::scalar(output), &[])?;
    let emit = |builder: &mut CircuitBuilderV5| -> Result<(), String> {
        let mut values = [[0; 128]; 3];
        for (i, dtype) in input_types.iter().enumerate() {
            let span = builder.input_byte_span(dtype.width())?;
            values[i] = builder.read_integer(&span, 0, *dtype)?;
        }
        let result = emit_scalar_v5(builder, prim, &values[..input_types.len()], output)?;
        let expected = builder.integer(expected);
        builder.assert_integer_equal(&result, &expected)
    };
    let mut counting = CircuitBuilderV5::preflight(limits)?;
    emit(&mut counting)?;
    let plan = counting.preflight_cost()?;
    let mut builder = CircuitBuilderV5::new_planned(limits, plan, memory)?;
    emit(&mut builder)?;
    builder.finish_reserved()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn caps() -> CircuitLimitsV5 {
        CircuitLimitsV5 {
            maximum_gates: 30_000_000,
            maximum_input_bits: 1024,
            maximum_assertions: 1_000_000,
            maximum_graph_bytes: 1_000_000_000,
            maximum_witness_bytes: 30_000_000,
        }
    }
    fn check(prim: misaka_palw_tir::Prim, types: &[DType], values: &[i128], output: DType, expected: i128) -> Result<(), String> {
        let memory = CircuitMemoryBudgetV5::new(1_100_000_000);
        let circuit = compile_scalar_v5(&prim, types, output, expected, caps(), memory.clone())?;
        let mut bytes = Vec::new();
        for (dtype, value) in types.iter().zip(values) {
            dtype.encode_le(*value, &mut bytes);
        }
        circuit.evaluate(&input_bits_v5(&bytes), caps()).map(|_| ())
    }
    #[test]
    fn signed_overflow_and_rounded_division_keep_reference_integer_semantics() {
        use misaka_palw_tir::Prim;
        assert!(check(Prim::Add, &[DType::I8, DType::I8], &[127, 1], DType::I8, -128).is_err());
        assert!(check(Prim::Add, &[DType::I128, DType::I128], &[i128::MAX, 1], DType::I128, i128::MIN).is_err());
        check(Prim::Sub, &[DType::I32, DType::I32], &[-9, 3], DType::I32, -12).unwrap();
        for rule in [Rounding::Floor, Rounding::HalfUp, Rounding::HalfAwayFromZero] {
            for (x, d) in [(-63, 2), (3, 2), (-64, 2), (i128::MIN, i128::MAX)] {
                check(
                    Prim::Div { rule },
                    &[DType::I128, DType::I128],
                    &[x, d],
                    DType::I128,
                    misaka_palw_tir::arith::div_round(x, d, rule).unwrap(),
                )
                .unwrap();
            }
        }
        assert!(check(Prim::Div { rule: Rounding::Floor }, &[DType::I32, DType::I32], &[5, 0], DType::I32, 0).is_err());
    }
    #[test]
    fn q24_scalar_circuits_match_frozen_reference_anchors() {
        use misaka_palw_tir::{Prim, arith::*};
        for x in [EXP_ZERO_AT, 0, ONE] {
            check(Prim::IntExp, &[DType::I32], &[x], DType::I32, int_exp(x)).unwrap();
        }
        for x in [0, ONE, 3 * ONE] {
            check(Prim::IntRsqrt, &[DType::I64], &[x], DType::I64, int_rsqrt(x)).unwrap();
        }
        for x in [0, ONE, 2 * ONE] {
            check(Prim::IntLn, &[DType::I64], &[x], DType::I64, int_ln(x)).unwrap();
        }
    }
}
