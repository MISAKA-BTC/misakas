//! Actual sparse rank-one constraints for the semantic/native-hash Boolean
//! graph. Rows are streamed from the immutable graph: no unreserved matrix or
//! field-witness copy is created. This is the algebraic relation required by a
//! polynomial IOP, not a PCS, a succinct argument or a security approval.
use super::*;
use crate::field::Fp;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct R1csLimitsV5 {
    pub maximum_rows: u64,
    pub maximum_variables: u64,
    pub maximum_nonzero_terms: u64,
    pub maximum_matrix_evaluation_work: u128,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct R1csShapeV5 {
    pub rows: u64,
    /// Variable zero is the public field constant one; gate g is variable g+1.
    pub variables: u64,
    pub nonzero_terms: u64,
    pub padded_rows: u64,
    pub padded_variables: u64,
    /// Multiplications for the streaming matrix MLE evaluator: one equality
    /// factor per row coordinate, one per term coordinate, and two per term.
    pub matrix_evaluation_work: u128,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct R1csTermV5 {
    pub variable: u64,
    pub coefficient: Fp,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct R1csLinearV5 {
    terms: [R1csTermV5; 3],
    len: u8,
}
impl R1csLinearV5 {
    fn new(terms: &[(u64, i128)]) -> Self {
        debug_assert!(terms.len() <= 3);
        let mut out = Self { terms: [R1csTermV5 { variable: 0, coefficient: Fp::ZERO }; 3], len: terms.len() as u8 };
        for (slot, (variable, coefficient)) in out.terms.iter_mut().zip(terms) {
            *slot = R1csTermV5 { variable: *variable, coefficient: Fp::from_i128(*coefficient) };
        }
        out
    }
    pub fn terms(&self) -> &[R1csTermV5] {
        &self.terms[..self.len as usize]
    }
    fn evaluate(&self, assignment: &mut impl FnMut(u64) -> Fp) -> Fp {
        self.terms().iter().fold(Fp::ZERO, |sum, term| sum.fadd(term.coefficient.fmul(assignment(term.variable))))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct R1csRowV5 {
    pub a: R1csLinearV5,
    pub b: R1csLinearV5,
    pub c: R1csLinearV5,
}
impl R1csRowV5 {
    pub fn satisfied(&self, mut assignment: impl FnMut(u64) -> Fp) -> bool {
        self.a.evaluate(&mut assignment).fmul(self.b.evaluate(&mut assignment)) == self.c.evaluate(&mut assignment)
    }
    fn terms(&self) -> u64 {
        u64::from(self.a.len) + u64::from(self.b.len) + u64::from(self.c.len)
    }
}
pub struct BooleanR1csV5<'a> {
    circuit: &'a BooleanCircuitV5,
    shape: R1csShapeV5,
}
fn gate_row(index: usize, gate: BooleanGateV5) -> R1csRowV5 {
    let output = index as u64 + 1;
    let wire = |id: WireV5| u64::from(id) + 1;
    let (a, b, c) = match gate {
        BooleanGateV5::Constant(false) => (R1csLinearV5::new(&[(output, 1)]), R1csLinearV5::new(&[(0, 1)]), R1csLinearV5::new(&[])),
        BooleanGateV5::Constant(true) => {
            (R1csLinearV5::new(&[(output, 1)]), R1csLinearV5::new(&[(0, 1)]), R1csLinearV5::new(&[(0, 1)]))
        }
        // Independently enforce b*(b-1)=0 even when a graph came through a
        // future wire decoder rather than this compiler's guarded input span.
        BooleanGateV5::Input { .. } => {
            (R1csLinearV5::new(&[(output, 1)]), R1csLinearV5::new(&[(output, 1), (0, -1)]), R1csLinearV5::new(&[]))
        }
        BooleanGateV5::And { a, b } => {
            (R1csLinearV5::new(&[(wire(a), 1)]), R1csLinearV5::new(&[(wire(b), 1)]), R1csLinearV5::new(&[(output, 1)]))
        }
        // XOR over any odd field: 2ab = a+b-output, with all inputs Boolean.
        BooleanGateV5::Xor { a, b } => (
            R1csLinearV5::new(&[(wire(a), 2)]),
            R1csLinearV5::new(&[(wire(b), 1)]),
            R1csLinearV5::new(&[(wire(a), 1), (wire(b), 1), (output, -1)]),
        ),
        BooleanGateV5::Not { a } => {
            (R1csLinearV5::new(&[(0, 1), (wire(a), -1)]), R1csLinearV5::new(&[(0, 1)]), R1csLinearV5::new(&[(output, 1)]))
        }
    };
    R1csRowV5 { a, b, c }
}
fn assertion_row(wire: WireV5, value: bool) -> R1csRowV5 {
    let a =
        if value { R1csLinearV5::new(&[(u64::from(wire) + 1, 1), (0, -1)]) } else { R1csLinearV5::new(&[(u64::from(wire) + 1, 1)]) };
    R1csRowV5 { a, b: R1csLinearV5::new(&[(0, 1)]), c: R1csLinearV5::new(&[]) }
}
impl<'a> BooleanR1csV5<'a> {
    pub fn new(circuit: &'a BooleanCircuitV5, graph_limits: CircuitLimitsV5, limits: R1csLimitsV5) -> Result<Self, String> {
        let cost = circuit.validate(graph_limits)?;
        let rows = cost.gates.checked_add(cost.assertions).ok_or("R1CS row count overflow")?;
        let variables = cost.gates.checked_add(1).ok_or("R1CS variable count overflow")?;
        if rows > limits.maximum_rows || variables > limits.maximum_variables {
            return Err("R1CS row/variable cap exceeded before lowering".into());
        }
        let mut terms = 0u64;
        for (index, gate) in circuit.gates().iter().enumerate() {
            terms = terms.checked_add(gate_row(index, *gate).terms()).ok_or("R1CS term count overflow")?;
            if terms > limits.maximum_nonzero_terms {
                return Err("R1CS term cap exceeded before lowering".into());
            }
        }
        for (wire, value) in circuit.assertions() {
            terms = terms.checked_add(assertion_row(*wire, *value).terms()).ok_or("R1CS assertion term overflow")?;
            if terms > limits.maximum_nonzero_terms {
                return Err("R1CS term cap exceeded before lowering".into());
            }
        }
        let padded_rows = rows.checked_next_power_of_two().ok_or("R1CS row padding overflow")?;
        let padded_variables = variables.checked_next_power_of_two().ok_or("R1CS variable padding overflow")?;
        let row_bits = padded_rows.ilog2();
        let column_bits = padded_variables.ilog2();
        let work = u128::from(rows)
            .checked_mul(u128::from(row_bits))
            .and_then(|n| n.checked_add(u128::from(terms).checked_mul(u128::from(column_bits) + 2)?))
            .ok_or("R1CS matrix evaluation work overflow")?;
        if work > limits.maximum_matrix_evaluation_work {
            return Err("R1CS matrix evaluation work cap exceeded".into());
        }
        Ok(Self {
            circuit,
            shape: R1csShapeV5 { rows, variables, nonzero_terms: terms, padded_rows, padded_variables, matrix_evaluation_work: work },
        })
    }
    pub fn shape(&self) -> R1csShapeV5 {
        self.shape
    }
    pub fn row(&self, index: u64) -> Option<R1csRowV5> {
        let gates = self.circuit.gates().len() as u64;
        if index < gates {
            return Some(gate_row(index as usize, self.circuit.gates()[index as usize]));
        }
        self.circuit
            .assertions()
            .get(usize::try_from(index.checked_sub(gates)?).ok()?)
            .map(|(wire, value)| assertion_row(*wire, *value))
    }
    /// Full reference assignment check for the prover/diagnostics. Its linear
    /// work is explicit and must not be advertised as compact verification.
    pub fn check_assignment(&self, assignment: &[u128]) -> Result<(), String> {
        if assignment.len() as u64 != self.shape.variables
            || assignment.first() != Some(&1)
            || assignment.iter().any(|value| Fp::from_canonical(*value).is_none())
        {
            return Err("R1CS assignment shape, constant-one or field encoding mismatch".into());
        }
        for index in 0..self.shape.rows {
            if !self
                .row(index)
                .ok_or("R1CS row missing")?
                .satisfied(|variable| Fp::from_canonical(assignment[variable as usize]).expect("canonical checked"))
            {
                return Err("R1CS constraint is not satisfied".into());
            }
        }
        Ok(())
    }
    /// Borrow the evaluator's retained witness. No field-sized Vec is cloned.
    pub fn check_boolean_witness(&self, witness: &CircuitWitnessV5) -> Result<(), String> {
        let values = witness.values();
        if values.len() != self.circuit.gates().len() {
            return Err("R1CS Boolean witness length mismatch".into());
        }
        for index in 0..self.shape.rows {
            if !self
                .row(index)
                .ok_or("R1CS row missing")?
                .satisfied(|variable| if variable == 0 || values[variable as usize - 1] { Fp::ONE } else { Fp::ZERO })
            {
                return Err("Boolean witness does not satisfy R1CS lowering".into());
            }
        }
        Ok(())
    }
    /// Exact sparse-matrix multilinear extensions for a polynomial IOP. This
    /// implementation scans every actual nonzero term; the returned cost states
    /// this work. Padded rows/columns are zero and allocate no backing vectors.
    pub fn matrix_evaluations(&self, row_point: &[Fp], column_point: &[Fp]) -> Result<[Fp; 3], String> {
        if row_point.len() != self.shape.padded_rows.ilog2() as usize
            || column_point.len() != self.shape.padded_variables.ilog2() as usize
        {
            return Err("R1CS matrix challenge dimension mismatch".into());
        }
        let mut result = [Fp::ZERO; 3];
        for index in 0..self.shape.rows {
            let weight = equality(index, row_point);
            let row = self.row(index).ok_or("R1CS row missing")?;
            for (matrix, linear) in [row.a, row.b, row.c].iter().enumerate() {
                for term in linear.terms() {
                    result[matrix] = result[matrix].fadd(weight.fmul(term.coefficient).fmul(equality(term.variable, column_point)));
                }
            }
        }
        Ok(result)
    }
}
fn equality(index: u64, point: &[Fp]) -> Fp {
    point.iter().enumerate().fold(Fp::ONE, |value, (bit, r)| value.fmul(if (index >> bit) & 1 == 0 { Fp::ONE.fsub(*r) } else { *r }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn graph_limits() -> CircuitLimitsV5 {
        CircuitLimitsV5 {
            maximum_gates: 1000,
            maximum_input_bits: 8,
            maximum_assertions: 100,
            maximum_graph_bytes: 100_000,
            maximum_witness_bytes: 1000,
        }
    }
    fn limits() -> R1csLimitsV5 {
        R1csLimitsV5 {
            maximum_rows: 1100,
            maximum_variables: 1001,
            maximum_nonzero_terms: 6000,
            maximum_matrix_evaluation_work: 100_000,
        }
    }
    fn graph() -> ReservedCircuitV5 {
        let memory = CircuitMemoryBudgetV5::new(200_000);
        let emit = |b: &mut CircuitBuilderV5| -> Result<(), String> {
            let input = b.input_byte_span(1)?;
            let bits = input.byte(0)?;
            let xor = b.xor(bits[0], bits[1])?;
            let and = b.and(bits[0], bits[1])?;
            let not = b.not(bits[2])?;
            b.assert(xor, true)?;
            b.assert(and, false)?;
            b.assert(not, false)
        };
        let mut count = CircuitBuilderV5::preflight(graph_limits()).unwrap();
        emit(&mut count).unwrap();
        let mut builder = CircuitBuilderV5::new_planned(graph_limits(), count.preflight_cost().unwrap(), memory).unwrap();
        emit(&mut builder).unwrap();
        builder.finish_reserved().unwrap()
    }
    #[test]
    fn sparse_field_rows_equal_boolean_semantics_and_refuse_all_altered_variables() {
        let graph = graph();
        let witness = graph.evaluate_bytes(&[5], graph_limits()).unwrap();
        let relation = BooleanR1csV5::new(graph.circuit(), graph_limits(), limits()).unwrap();
        relation.check_boolean_witness(&witness).unwrap();
        let mut assignment = vec![1];
        assignment.extend(witness.values().iter().map(|v| u128::from(*v)));
        relation.check_assignment(&assignment).unwrap();
        for variable in 0..assignment.len() {
            let before = assignment[variable];
            assignment[variable] = 2;
            assert!(relation.check_assignment(&assignment).is_err());
            assignment[variable] = before;
        }
        assignment[1] = Fp::MODULUS;
        assert!(relation.check_assignment(&assignment).is_err());
    }
    #[test]
    fn matrix_mle_at_boolean_points_is_the_exact_sparse_coefficient() {
        let graph = graph();
        let relation = BooleanR1csV5::new(graph.circuit(), graph_limits(), limits()).unwrap();
        for index in 0..relation.shape.padded_rows {
            let row = relation.row(index);
            for variable in 0..relation.shape.padded_variables {
                let point = |value: u64, bits: u32| {
                    (0..bits).map(|i| if value >> i & 1 == 0 { Fp::ZERO } else { Fp::ONE }).collect::<Vec<_>>()
                };
                let actual = relation
                    .matrix_evaluations(
                        &point(index, relation.shape.padded_rows.ilog2()),
                        &point(variable, relation.shape.padded_variables.ilog2()),
                    )
                    .unwrap();
                let expected = row
                    .map(|row| {
                        [row.a, row.b, row.c].map(|lc| {
                            lc.terms().iter().filter(|t| t.variable == variable).fold(Fp::ZERO, |sum, t| sum.fadd(t.coefficient))
                        })
                    })
                    .unwrap_or([Fp::ZERO; 3]);
                assert_eq!(actual, expected);
            }
        }
        let mut tiny = limits();
        tiny.maximum_nonzero_terms = 0;
        assert!(BooleanR1csV5::new(graph.circuit(), graph_limits(), tiny).is_err());
    }
}
