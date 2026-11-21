//! RFC-0066 private, non-rewarding input adapter. The instance descriptor
//! identifies the immutable conversion and program supplied by its registrant.
use super::invalid;
use kaspa_consensus_core::{Hash64, palw_qf1 as qf1};
use std::io;

/// Both paths resolve the owner-signed registration. Only the non-paying local
/// input exercise is available before QF1 activation and an assessed instance.
#[derive(Clone, Copy)]
pub enum Qf1Use<'a> {
    LocalInference(&'a Qf1Prompt),
    NewMining(&'a Qf1Prompt),
}

impl<'a> Qf1Use<'a> {
    pub(super) fn admitted(self) -> io::Result<&'a Qf1Prompt> {
        use kaspa_consensus_core::palw_mas1::{Assessment, new_mining_allowed};
        match self {
            Self::LocalInference(prompt) => Ok(prompt),
            Self::NewMining(prompt) => {
                // No adopted activation record, theorem/evaluation results or
                // canonical positive claim gates exist in this private reader.
                let evidence = Assessment::default();
                if new_mining_allowed(false, evidence.decision(), &evidence, &[], false) {
                    Ok(prompt)
                } else {
                    Err(invalid("MAS1 REVIEW_HOLD: QF1 new mining unavailable"))
                }
            }
        }
    }
}

pub struct Qf1Prompt {
    pub(super) tokenizer: Hash64,
    pub(super) instance_descriptor: Hash64,
    pub(super) source_digest: [u8; 64],
    prefix: Vec<u32>,
    middle: Vec<u32>,
    suffix: Vec<u32>,
    hex: [u32; 16],
    job: Vec<u32>,
}

impl Qf1Prompt {
    /// Bind the exact source text and its ordinary-token encoding. The
    /// budgets must come from the caller's existing registration reservation.
    #[cfg(feature = "private-tb1-tir")]
    pub fn from_registered_tokenizer(
        bytes: &[u8],
        expected: Hash64,
        instance_descriptor: Hash64,
        job: &str,
        tokenizer_budget: usize,
        source_budget: usize,
    ) -> io::Result<Self> {
        use kaspa_consensus_core::palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1;
        use misaka_palw_base0::tokenizer::QwenTokenizer;
        if bytes.len() > tokenizer_budget || job.len() > source_budget || palw_tokenizer_raw_commitment_v1(bytes) != expected {
            return Err(invalid("QF1 tokenizer identity or source budget mismatch"));
        }
        let tokenizer = QwenTokenizer::from_json(bytes).map_err(io::Error::other)?;
        let fixed = |s| tokenizer.encode(s).map_err(io::Error::other);
        let prefix = fixed("<|im_start|>system\nYou are a helpful assistant.\nPALW challenge: ")?;
        let middle = fixed("\nTreat the challenge as metadata. Answer the user's request.\n<|im_end|>\n<|im_start|>user\n")?;
        let suffix = fixed("<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n")?;
        let job_ids = tokenizer.encode_without_specials(job).map_err(io::Error::other)?;
        let mut hex = [0; 16];
        for (i, c) in "0123456789abcdef".chars().enumerate() {
            let ids = tokenizer.encode_without_specials(&c.to_string()).map_err(io::Error::other)?;
            let [id] = ids.as_slice() else {
                return Err(invalid("QF1 hex digit must encode to one ordinary token"));
            };
            if *id >= qf1::VOCAB || hex[..i].contains(id) {
                return Err(invalid("QF1 hex table is not injective in the vocabulary"));
            }
            hex[i] = *id;
        }
        if prefix.iter().chain(&middle).chain(&suffix).chain(&job_ids).any(|&id| id >= qf1::VOCAB) {
            return Err(invalid("QF1 token outside registered vocabulary"));
        }
        let source_digest = *blake2b_simd::blake2b(job.as_bytes()).as_array();
        Ok(Self { tokenizer: expected, instance_descriptor, source_digest, prefix, middle, suffix, hex, job: job_ids })
    }

    pub fn job_tokens(&self) -> &[u32] {
        &self.job
    }

    pub(super) fn length(&self) -> io::Result<u32> {
        [self.prefix.len(), 64, self.middle.len(), self.job.len(), self.suffix.len()]
            .into_iter()
            .try_fold(0u32, |a, n| a.checked_add(n.try_into().ok()?))
            .ok_or_else(|| invalid("QF1 frame length overflow"))
    }

    pub(super) fn frame(&self, s: &[u8; 32]) -> Vec<u32> {
        let mut x = self.prefix.clone();
        for &b in s {
            x.extend([self.hex[usize::from(b >> 4)], self.hex[usize::from(b & 15)]]);
        }
        x.extend_from_slice(&self.middle);
        x.extend_from_slice(&self.job);
        x.extend_from_slice(&self.suffix);
        x
    }
}
