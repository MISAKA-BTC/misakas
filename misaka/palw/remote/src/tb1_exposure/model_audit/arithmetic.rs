//! Two bounded private exact-answer tasks. Scores are local observations until
//! the evaluation submission is closed and independently replayed by its PANEL.
use super::*;
use crate::tb1_exposure::exec_input::LocalOriginalBody;
use crate::tb1_exposure::qf1::Qf1Prompt;
use kaspa_consensus_core::{
    config::params::Params,
    palw_execution_lane_v1::palw_execution_span_v1,
    palw_mode_v2::PalwConsensusMode,
    palw_model_registry_v1::PalwModelRegistryFoldV1,
    palw_panel_v2::{PALW_DRAW_WEIGHT_MAX_MSK_V1, PalwPanelStakeDrawV1, palw_panel_stake_operator_weights_v1},
    palw_qf1::{Stop, is_eos},
    palw_state_v2::{PalwChainStateV2, palw_base_population_v1},
};
use misaka_palw_tir_exec::node::qf1::{LocalInstance, TraceRequest};

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

/// Public input for later independent replay. The suite determines the expected
/// digit from the challenge; evaluators derive the score from the output tokens.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, borsh::BorshSerialize)]
pub struct ArithmeticEvaluation {
    pub case: u32,
    pub challenge: [u8; 32],
    pub job: String,
    pub expected: u32,
    pub output: Vec<u32>,
    pub stopped_at_eos: bool,
    pub positions: u32,
    pub z: Hash64,
    pub original: LocalOriginalBody,
}

/// Commit both cases in suite order on the branch that published this audit seed.
pub fn submission_marker(certificate: Hash64, cases: &[ArithmeticEvaluation; CASES as usize]) -> io::Result<[u8; 64]> {
    for (i, row) in cases.iter().enumerate() {
        if row.case != i as u32
            || row.output.is_empty()
            || row.output.len() > LIMIT as usize
            || row.job != (ArithmeticCase { challenge: row.challenge, index: row.case }).job().0
        {
            return Err(invalid("incomplete or unordered arithmetic submission"));
        }
    }
    Ok(Sha3_512::new()
        .chain_update(b"MISAKA/private/MAS1/arithmetic-submission/v1")
        .chain_update(suite_id().as_bytes())
        .chain_update(certificate.as_bytes())
        .chain_update(borsh::to_vec(cases)?)
        .finalize()
        .into())
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

    fn execute(&self, model: &LocalInstance<'_>, tokenizer: &[u8], submitted_length: Option<u32>) -> io::Result<ArithmeticEvaluation> {
        let id = model.identity();
        let (job, digit) = self.job();
        let prompt = Qf1Prompt::from_registered_tokenizer(tokenizer, id.tokenizer, id.descriptor, &job, 32 << 20, 256)?;
        let tokens = prompt.frame(&self.challenge);
        let expected = prompt.digit_token(digit);
        let suite = suite_id();
        let trace = match submitted_length {
            None => model.produce_trace(&tokens, LIMIT, LEAVES, suite, self.challenge, suite, None),
            Some(answer_len) => model.replay_trace(
                &tokens,
                LIMIT,
                LEAVES,
                suite,
                self.challenge,
                TraceRequest { context_digest: suite, answer_len, open: None },
            ),
        }
        .map_err(io::Error::other)?;
        let original = LocalOriginalBody::from_trace(&trace);
        Ok(ArithmeticEvaluation {
            case: self.index,
            challenge: self.challenge,
            job,
            expected,
            output: trace.answer.tokens,
            stopped_at_eos: trace.answer.stop == Stop::Eos,
            positions: trace.answer.processed_positions,
            z: Hash64::from_bytes(trace.answer.z),
            original,
        })
    }
}

pub fn exact_digit_score(expected: u32, output: &[u32]) -> bool {
    matches!(output, [digit, eos] if *digit == expected && is_eos(*eos))
}

/// Commit the sorted, weighted operators resolved by the control policy
/// against the selected parent's post-state.
fn arithmetic_control_roster(state: Hash64, operators: &[(Hash64, u64)]) -> io::Result<[u8; 64]> {
    if state == Hash64::default()
        || operators.is_empty()
        || operators
            .iter()
            .any(|(operator, weight)| *operator == Hash64::default() || !(1..=PALW_DRAW_WEIGHT_MAX_MSK_V1).contains(weight))
        || operators.windows(2).any(|pair| pair[0].0 >= pair[1].0)
    {
        return Err(invalid("invalid arithmetic PANEL control roster"));
    }
    let count = u32::try_from(operators.len()).map_err(|_| invalid("oversized arithmetic PANEL control roster"))?;
    let mut h = Sha3_512::new()
        .chain_update(b"MISAKA/private/MAS1/arithmetic-panel-roster/v1")
        .chain_update(state.as_bytes())
        .chain_update(count.to_be_bytes());
    for (operator, weight) in operators {
        h.update(operator.as_bytes());
        h.update(weight.to_be_bytes());
    }
    Ok(h.finalize().into())
}

/// The native observer supplies the selected parent's post-state and the
/// registry policy resolved at that parent.
pub struct ArithmeticControlView<'a> {
    pub state: &'a PalwChainStateV2,
    pub config: &'a Params,
    pub registry: &'a PalwModelRegistryFoldV1,
}

impl ArithmeticControlView<'_> {
    fn roster(&self, class: Hash64, daa: u64) -> io::Result<(Hash64, [u8; 64])> {
        let PalwConsensusMode::ConsensusV2(bundle) = &self.config.palw_consensus_mode else {
            return Err(invalid("arithmetic PANEL needs a native control registry"));
        };
        let span = palw_execution_span_v1(daa, self.registry.span_daa);
        let activation_pool = self.config.palw_activation_pool_at(daa).is_some();
        let registered = self.state.class(&class).ok_or_else(|| invalid("arithmetic candidate absent at PANEL parent"))?;
        let owner = registered
            .registrant_bond
            .map(|key| self.state.bond(&key).ok_or_else(|| invalid("arithmetic registrant bond absent at PANEL parent")))
            .transpose()?;
        let eligible = palw_base_population_v1(self.state, &bundle.state, self.registry, &class, daa, span, activation_pool);
        let bonds = eligible.into_iter().map(|(_, bond)| bond).filter(|bond| owner.is_none_or(|owner| owner.pubkey != bond.pubkey));
        let operators: Vec<_> = palw_panel_stake_operator_weights_v1(bonds, &PalwPanelStakeDrawV1::V1).into_iter().collect();
        let root = self.state.state_root();
        Ok((root, arithmetic_control_roster(root, &operators)?))
    }
}

/// Private assessment close prepared against a single selected parent.
pub struct ArithmeticPanelCut {
    parent: Hash64,
    control_state: Hash64,
    closure: Tb1MiningClosure,
}

impl ArithmeticPanelCut {
    pub fn commitment(&self) -> [u8; 64] {
        self.closure.commitment(self.parent)
    }

    pub fn holder_freeze(&self, native: &dyn ConsensusApi, close: Hash64, q: &Tb1Qualification<'_>) -> io::Result<Tb1HolderFreeze> {
        let tip = native.get_sink();
        if native.get_ghostdag_data(close).map_err(io::Error::other)?.selected_parent != self.parent
            || !native.is_chain_ancestor_of(close, tip).map_err(io::Error::other)?
        {
            return Err(invalid("arithmetic PANEL close is outside the prepared selected branch"));
        }
        let header = native.get_header(close).map_err(io::Error::other)?;
        if state_root_of_pinned_header_v1(&header, close).map_err(io::Error::other)? != self.control_state {
            return Err(invalid("arithmetic PANEL close commits a different control state"));
        }
        let freeze = Tb1HolderFreeze::resolve(native, close, &self.closure, q)?;
        if native.get_sink() != tip {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Ok(freeze)
    }
}

impl FrozenAudit<'_> {
    fn check_arithmetic_model(&self, candidate: u32, model: &LocalInstance<'_>) -> io::Result<()> {
        if self.cut.plan.suite != suite_id() || self.cut.plan.tests_per_candidate != CASES {
            return Err(invalid("different frozen arithmetic suite or case count"));
        }
        let admitted = self.cut.candidates.get(candidate as usize).ok_or_else(|| invalid("unknown audit candidate"))?;
        let id = model.identity();
        if (id.class, id.inventory, id.descriptor) != (admitted.class, admitted.artifact, admitted.descriptor) {
            return Err(invalid("different model from frozen arithmetic candidate"));
        }
        Ok(())
    }

    /// Authenticate the recorded cases for this candidate from the original release
    /// certificate. The submission block determines timeliness even on a later tip.
    #[allow(clippy::too_many_arguments)]
    pub fn read_arithmetic_submission(
        &self,
        native: &dyn ConsensusApi,
        batch: Hash64,
        certificate: Hash64,
        releases: &[PublishedRelease<'_>],
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
        candidate: u32,
        submission: Hash64,
        originals: &[ArithmeticEvaluation; CASES as usize],
    ) -> io::Result<[u8; 64]> {
        if self.cut.plan.suite != suite_id()
            || self.cut.plan.tests_per_candidate != CASES
            || candidate as usize >= self.cut.candidates.len()
        {
            return Err(invalid("submission outside the frozen arithmetic suite"));
        }
        let tip = native.get_sink();
        let seed = self.gate.read_seed(native, batch, certificate, releases, q, limits)?;
        for (index, row) in originals.iter().enumerate() {
            if row.challenge != self.case_challenge(&seed, candidate, index as u32) {
                return Err(invalid("submission differs from the certified arithmetic draw"));
            }
        }
        let marker = self.read_arithmetic_marker(native, certificate, submission, originals)?;
        if native.get_sink() != tip {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Ok(marker)
    }

    /// Prepare the scheduled assessment close before PANEL release. The
    /// enclosing policy must derive the control population from the supplied
    /// parent. Its commitment covers the eligible operators and draw weights.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_arithmetic_panel(
        &self,
        native: &dyn ConsensusApi,
        batch: Hash64,
        certificate: Hash64,
        releases: &[PublishedRelease<'_>],
        audit_q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
        candidate: u32,
        submission: Hash64,
        originals: &[ArithmeticEvaluation; CASES as usize],
        panel_q: &Tb1Qualification<'_>,
        parent: Hash64,
        control: ArithmeticControlView<'_>,
        draw_policy: Hash64,
    ) -> io::Result<ArithmeticPanelCut> {
        let round = self.cut.assessment_panel(panel_q)?;
        if native.get_sink() != parent {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let h = native.get_header(parent).map_err(io::Error::other)?;
        if h.blue_score.checked_add(1) != Some(round.close_blue_score)
            || h.daa_score >= round.release_from_daa
            || draw_policy == Hash64::default()
        {
            return Err(invalid("invalid arithmetic PANEL parent or control policy"));
        }
        let admitted = self.cut.candidates.get(candidate as usize).ok_or_else(|| invalid("unknown audit candidate"))?;
        let (control_state, control_roster) = control.roster(admitted.class, h.daa_score)?;
        let marker =
            self.read_arithmetic_submission(native, batch, certificate, releases, audit_q, limits, candidate, submission, originals)?;
        if native.get_sink() != parent {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let consumer_set_root = Sha3_512::new()
            .chain_update(b"MISAKA/private/MAS1/arithmetic-panel-inputs/v1")
            .chain_update(self.cut.closure.consumer_set_root)
            .chain_update(certificate.as_bytes())
            .chain_update(candidate.to_be_bytes())
            .chain_update(submission.as_bytes())
            .chain_update(marker)
            .chain_update(control_roster)
            .finalize()
            .into();
        let closure = Tb1MiningClosure { round, qual_root: panel_q.root(), consumer_set_root, policy_root: draw_policy.as_bytes() };
        Ok(ArithmeticPanelCut { parent, control_state, closure })
    }

    fn read_arithmetic_marker(
        &self,
        native: &dyn ConsensusApi,
        certificate: Hash64,
        submission: Hash64,
        originals: &[ArithmeticEvaluation; CASES as usize],
    ) -> io::Result<[u8; 64]> {
        let marker = submission_marker(certificate, originals)?;
        let tip = native.get_sink();
        let parent = native.get_ghostdag_data(submission).map_err(io::Error::other)?.selected_parent;
        let header = native.get_header(submission).map_err(io::Error::other)?;
        if parent != certificate
            || header.daa_score >= self.cut.plan.case_deadline_exclusive_daa
            || header.blue_score >= self.cut.panel.close_blue_score
            || !native.is_chain_ancestor_of(submission, tip).map_err(io::Error::other)?
            || !native.palw_coinbase_commits_to(self.cut.closure.round.chain_id, submission, &marker).map_err(io::Error::other)?
        {
            return Err(invalid("arithmetic submission outside the original assessment cut"));
        }
        if native.get_sink() != tip {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Ok(marker)
    }

    /// Independently stream the two numerical traces against the Original carried
    /// on the selected branch. Only their lengths are supplied to the executor.
    #[allow(clippy::too_many_arguments)]
    pub fn replay_arithmetic_submission(
        &self,
        native: &dyn ConsensusApi,
        batch: Hash64,
        certificate: Hash64,
        releases: &[PublishedRelease<'_>],
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
        candidate: u32,
        submission: Hash64,
        originals: &[ArithmeticEvaluation; CASES as usize],
        model: &LocalInstance<'_>,
        tokenizer: &[u8],
    ) -> io::Result<()> {
        self.check_arithmetic_model(candidate, model)?;
        let check_receipt = || self.read_arithmetic_marker(native, certificate, submission, originals);
        for row in originals {
            check_receipt()?;
            let challenge = || self.challenge(native, batch, certificate, releases, q, limits, candidate, row.case);
            let expected = challenge()?;
            if expected != row.challenge {
                return Err(invalid("arithmetic submission differs from drawn challenge"));
            }
            let length = u32::try_from(row.output.len()).map_err(|_| invalid("arithmetic output length"))?;
            let actual = ArithmeticCase { challenge: expected, index: row.case }.execute(model, tokenizer, Some(length))?;
            if actual != *row || challenge()? != expected {
                return Err(invalid("arithmetic re-execution differs from submitted Original"));
            }
        }
        check_receipt().map(|_| ())
    }

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
    ) -> io::Result<ArithmeticEvaluation> {
        self.check_arithmetic_model(candidate, model)?;
        let current = || self.challenge(native, batch, certificate, rows, q, limits, candidate, case);
        let challenge = current()?;
        let result = ArithmeticCase { challenge, index: case }.execute(model, tokenizer, None)?;
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
    fn submission_binds_originals_in_draw_order() {
        let certificate = Hash64::from_bytes([4; 64]);
        let mut rows = std::array::from_fn(|index| {
            let case = ArithmeticCase { challenge: [7; 32], index: index as u32 };
            ArithmeticEvaluation {
                case: case.index,
                challenge: case.challenge,
                job: case.job().0,
                expected: 19,
                output: vec![19, 151643],
                stopped_at_eos: true,
                positions: 80,
                z: Hash64::default(),
                original: LocalOriginalBody { counts: [79, 2, 1, 900], roots: [Hash64::default(); 4], ticket: [0; 32] },
            }
        });
        let original = submission_marker(certificate, &rows).unwrap();
        // Independent Borsh encoding in the public arithmetic evidence reader.
        assert_eq!(
            Hash64::from_bytes(original).to_string(),
            "af3d3a0f952481f20a1db63720686d9a3b93552acafa6e84ed59159bb3ab99621fb78adde6771f8ddeda6c4b1256ae6743012cd758582ec852163948a9c7005d"
        );
        assert_ne!(submission_marker(Hash64::default(), &rows).unwrap(), original);
        rows.swap(0, 1);
        assert!(submission_marker(certificate, &rows).is_err());
        rows.swap(0, 1);
        rows[1].original.roots[2] = certificate;
        assert_ne!(submission_marker(certificate, &rows).unwrap(), original);
        rows[1].original.roots[2] = Hash64::default();
        rows[1].output[0] += 1;
        assert_ne!(submission_marker(certificate, &rows).unwrap(), original);
    }

    #[test]
    fn assessment_roster_binds_parent_state_and_exact_operator_weights() {
        let state = Hash64::from_u64_word(1);
        let a = Hash64::from_u64_word(2);
        let b = Hash64::from_u64_word(3);
        let roster = arithmetic_control_roster(state, &[(a, 10), (b, 20)]).unwrap();
        assert_ne!(arithmetic_control_roster(b, &[(a, 10), (b, 20)]).unwrap(), roster);
        assert_ne!(arithmetic_control_roster(state, &[(a, 10), (b, 21)]).unwrap(), roster);
        for invalid in
            [vec![], vec![(a, 0)], vec![(a, PALW_DRAW_WEIGHT_MAX_MSK_V1 + 1)], vec![(a, 10), (a, 20)], vec![(b, 20), (a, 10)]]
        {
            assert!(arithmetic_control_roster(state, &invalid).is_err());
        }
    }

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
