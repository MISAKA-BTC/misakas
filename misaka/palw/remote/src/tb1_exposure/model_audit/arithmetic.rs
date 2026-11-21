//! Two bounded private exact-answer tasks. Scores are local observations until
//! the evaluation submission is closed and independently replayed by its PANEL.
use super::*;
use crate::tb1_exposure::qf1::Qf1Prompt;
use kaspa_consensus_core::palw_qf1::is_eos;
use misaka_palw_tir_exec::node::qf1::{LocalAnswer, LocalInstance};

pub const CASES: u32 = 2;
const LIMIT: u32 = 16;
const LEAVES: u64 = 32_000_000;
const PREFIX: &str = "Answer with only one digit: what is ";
const SUFFIX: &str = "?";

/// The suite fixes both generators, frame, scoring rule and execution bounds
/// before any challenge is drawn. Candidate identity is bound by the outer cut.
pub fn suite_id() -> Hash64 {
    Hash64::from_bytes(
        Sha3_512::new()
            .chain_update(b"MISAKA/private/MAS1/arithmetic/v1\0HEX64\0a=c[0]%5,b=c[1]%5\0add(a,b),sub(a+b,b)\0")
            .chain_update(PREFIX.as_bytes())
            .chain_update(b"\0 + \0 - \0")
            .chain_update(SUFFIX.as_bytes())
            .chain_update(b"\0canonical-digit-token,one-QF1-EOS\0required-correct=2\0")
            .chain_update(CASES.to_be_bytes())
            .chain_update(LIMIT.to_be_bytes())
            .chain_update(LEAVES.to_be_bytes())
            .finalize()
            .into(),
    )
}

struct ArithmeticCase {
    challenge: [u8; 32],
    index: u32,
}

impl ArithmeticCase {
    fn job(&self) -> (String, u8) {
        let a = self.challenge[0] % 5;
        let b = self.challenge[1] % 5;
        if self.index == 0 { (format!("{PREFIX}{a} + {b}{SUFFIX}"), a + b) } else { (format!("{PREFIX}{} - {b}{SUFFIX}", a + b), a) }
    }

    fn execute(&self, model: &LocalInstance<'_>, tokenizer: &[u8]) -> io::Result<(String, u32, LocalAnswer)> {
        let id = model.identity();
        let (job, digit) = self.job();
        let prompt = Qf1Prompt::from_registered_tokenizer(tokenizer, id.tokenizer, id.descriptor, &job, 32 << 20, 256)?;
        let tokens = prompt.frame(&self.challenge);
        let expected = prompt.digit_token(digit);
        let answer = model.replay(&tokens, LIMIT, LEAVES, suite_id(), self.challenge).map_err(io::Error::other)?;
        Ok((job, expected, answer))
    }
}

pub fn exact_digit_score(expected: u32, output: &[u32]) -> bool {
    matches!(output, [digit, eos] if *digit == expected && is_eos(*eos))
}

impl FrozenAudit<'_> {
    /// Recheck the selected branch, certificate, candidate and original deadline
    /// on both sides of inference. No local score grants admission or credit.
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate_arithmetic(
        &self,
        native: &dyn ConsensusApi,
        batch: Hash64,
        certificate: Hash64,
        rows: &[PublishedRelease<'_>],
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
        candidate: u32,
        case: u32,
        model: &LocalInstance<'_>,
        tokenizer: &[u8],
    ) -> io::Result<(String, u32, LocalAnswer)> {
        if self.cut.plan.suite != suite_id() || self.cut.plan.tests_per_candidate != CASES {
            return Err(invalid("different frozen arithmetic suite or case count"));
        }
        let admitted = self.cut.candidates.get(candidate as usize).ok_or_else(|| invalid("unknown audit candidate"))?;
        let id = model.identity();
        if (id.class, id.inventory, id.descriptor) != (admitted.class, admitted.artifact, admitted.descriptor) {
            return Err(invalid("different model from frozen arithmetic candidate"));
        }
        let current = || self.challenge(native, batch, certificate, rows, q, limits, candidate, case);
        let challenge = current()?;
        let result = ArithmeticCase { challenge, index: case }.execute(model, tokenizer)?;
        if current()? != challenge {
            return Err(invalid("arithmetic challenge changed during evaluation"));
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arithmetic_generators_and_exact_scores() {
        let mut challenge = [0; 32];
        challenge[..2].copy_from_slice(&[254, 253]);
        assert_eq!(ArithmeticCase { challenge, index: 0 }.job(), ("Answer with only one digit: what is 4 + 3?".into(), 7));
        assert_eq!(ArithmeticCase { challenge, index: 1 }.job(), ("Answer with only one digit: what is 7 - 3?".into(), 4));
        for eos in [151643, 151645] {
            assert!(exact_digit_score(22, &[22, eos]));
        }
        for output in [&[][..], &[22], &[23, 151643], &[22, 22, 151645], &[22, 42]] {
            assert!(!exact_digit_score(22, output));
        }
    }
}
