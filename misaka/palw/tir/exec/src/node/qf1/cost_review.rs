//! Offline differential verification for two already recorded QF1 Originals.
//! Both traces keep their own attempt, Init, tree and End. No cached state is
//! accepted from outside the invocation and no claim or ticket is returned.
use super::{LocalAnswer, LocalInstance, LocalTrace, Trace, TraceRequest};
use crate::{NoSink, NodeValue, StepSink, TirExecutor};
use kaspa_consensus_core::{Hash64, palw_qf1};
use misaka_palw_tir::{TirError, TirErrorKind, TirResult};

pub struct ArchivedOriginal {
    pub attempt: [u8; 32],
    pub context: Hash64,
    pub prompt: Vec<u32>,
    pub answer: Vec<u32>,
    pub z: [u8; 64],
    pub roots: [[u8; 64]; 4],
    pub ticket: [u8; 32],
}

impl ArchivedOriginal {
    fn request(&self, discovered: LocalAnswer) -> Result<TraceRequest, String> {
        if discovered.tokens != self.answer || discovered.z != self.z {
            return Err("first numerical pass differs from recorded Original".into());
        }
        Ok(TraceRequest {
            context_digest: self.context,
            answer_len: discovered.tokens.len().try_into().map_err(|_| "QF1 answer length")?,
            open: None,
        })
    }

    fn compare(&self, trace: LocalTrace) -> Result<(), String> {
        if self.answer != trace.answer.tokens
            || self.z != trace.answer.z
            || self.roots != trace.roots.map(|root| root.digest)
            || self.ticket != trace.ticket
        {
            return Err("cost replay differs from recorded answer, numerical readout, roots or ticket".into());
        }
        Ok(())
    }
}

struct SharedValues<'a> {
    position: u32,
    traces: &'a mut [Trace; 2],
}
impl StepSink for SharedValues<'_> {
    fn node(&mut self, _: &NodeValue<'_>) {
        unreachable!("executor calls the fallible observer")
    }
    fn try_node(&mut self, value: &NodeValue<'_>) -> TirResult<()> {
        for trace in &mut *self.traces {
            trace.node_value(self.position, value).map_err(|e| TirError::new(TirErrorKind::Operand, e))?;
        }
        Ok(())
    }
}

impl LocalInstance<'_> {
    pub fn verify_independent_original(&self, original: &ArchivedOriginal, limit: u32, cap: u64) -> Result<(), String> {
        let discovered = self.replay(&original.prompt, limit, cap, original.context, original.attempt)?;
        original.compare(self.replay_trace(
            &original.prompt,
            limit,
            cap,
            original.context,
            original.attempt,
            original.request(discovered)?,
        )?)
    }

    /// Measure a length prediction on an archived Original. The numerical run
    /// still determines every token and Stop. Include the complete independent
    /// retry when the prediction misses and retain the failed trace diagnostic.
    /// The caller times both paths together.
    pub fn verify_predicted_length(
        &self,
        original: &ArchivedOriginal,
        predicted: u32,
        limit: u32,
        cap: u64,
    ) -> Result<Option<String>, String> {
        if predicted == 0 || predicted > limit {
            return Err("predicted length outside QF1 decode budget".into());
        }
        let request = TraceRequest { context_digest: original.context, answer_len: predicted, open: None };
        match self.replay_trace(&original.prompt, limit, cap, original.context, original.attempt, request) {
            Ok(trace) => {
                original.compare(trace)?;
                Ok(None)
            }
            Err(error) if original.answer.len() == predicted as usize => Err(error),
            Err(error) => {
                self.verify_independent_original(original, limit, cap)?;
                Ok(Some(error))
            }
        }
    }

    /// Compute the common prefill once, while feeding both originals every
    /// committed value. Export/import and each attempt-specific rehash are
    /// part of the measured route. The last prefill position predicts a token
    /// and is always evaluated independently.
    pub fn verify_shared_original_pair(&self, pair: [&ArchivedOriginal; 2], limit: u32, cap: u64) -> Result<usize, String> {
        let [first, second] = pair;
        if first.attempt == second.attempt || first.prompt == second.prompt {
            return Err("independent QF1 attempts and inputs required".into());
        }
        self.artifact.check_weights_unmodified()?;
        for original in pair {
            let len = u32::try_from(original.prompt.len()).map_err(|_| "QF1 input length")?;
            palw_qf1::check_capacity(len, limit)?;
            if original.prompt.iter().any(|&token| token >= palw_qf1::VOCAB) {
                return Err("QF1 input ID out of range".into());
            }
        }
        let prefix = first.prompt[..first.prompt.len() - 1]
            .iter()
            .zip(&second.prompt[..second.prompt.len() - 1])
            .take_while(|(a, b)| a == b)
            .count();
        // Discover T from the numerical run before allocating its trace. The
        // archived answer is used only to check the result of each pass.
        let prefill = |mut traces: Option<&mut [Trace; 2]>| {
            let mut exec = TirExecutor::new(self.artifact.plan(), self.artifact.params()).map_err(|e| e.to_string())?;
            exec.set_fused(true);
            for &token in &first.prompt[..prefix] {
                let pos = exec.pos();
                let result = match traces.as_deref_mut() {
                    Some(t) => exec.step_opt(token, &mut SharedValues { position: pos, traces: t }, false),
                    None => exec.step_opt(token, &mut NoSink, false),
                };
                result.map_err(|e| e.to_string())?;
            }
            Ok::<_, String>(exec.export_state())
        };
        let requests = {
            let state = prefill(None)?;
            let discover =
                |o: &ArchivedOriginal| o.request(self.drive(&o.prompt, limit, cap, o.context, o.attempt, None, Some(&state))?);
            [discover(first)?, discover(second)?]
        };
        let begin = |o: &ArchivedOriginal, request| Trace::begin(self, &o.prompt, limit, cap, o.attempt, request);
        let mut traces = [begin(first, requests[0])?, begin(second, requests[1])?];
        let state = prefill(Some(&mut traces))?;
        for (original, mut trace) in pair.into_iter().zip(traces) {
            let answer =
                self.drive(&original.prompt, limit, cap, original.context, original.attempt, Some(&mut trace), Some(&state))?;
            original.compare(trace.finish(answer)?)?;
        }
        Ok(prefix)
    }
}
