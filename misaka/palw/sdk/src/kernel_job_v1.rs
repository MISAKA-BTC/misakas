//! **Lane J1: the user's kernel job, text in and text out** — what `misaka palw job post` and `misaka palw job result` do with the
//! user's tokenizer, kept here so the CLI and the node E2E share one spelling.
//!
//! * [`kernel_tokenizer_v1`]: load a `tokenizer.json` the user supplies. When the class commits to a tokenizer, the file must hash to
//!   that commitment ([`palw_tokenizer_raw_commitment_v1`]); a kernel class binds none (`tokenizer_or_input_schema_root = 0`), and then
//!   the file is used as given and its digest is only reported — nothing is reviewed or recorded on chain.
//! * [`kernel_job_from_text_v1`]: tokenise the prompt (no control tokens from text) and build the `KernelJobV1` the ledger's own
//!   `well_formed` accepts (token bound, positions).
//! * [`kernel_result_text_v1`]: render a claim's generated ids back to text.
//!
//! This is client-side convenience only. The chain judges token ids; the text is whatever the user's tokenizer makes of them.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1;
use misaka_palw_base0::tokenizer::QwenTokenizer;
use misaka_palw_kernel::job::{DecodeRuleV1, KernelJobV1};

/// A loaded tokenizer and its raw digest (what a commitment would be over).
pub struct KernelTokenizerV1 {
    pub tokenizer: QwenTokenizer,
    pub digest: Hash64,
    /// `true` only when the class committed to a tokenizer and the file matched it.
    pub verified: bool,
}

/// Load `bytes` as a `tokenizer.json`; when `commitment` is `Some` (and non-zero), the file must hash to it.
pub fn kernel_tokenizer_v1(bytes: &[u8], commitment: Option<Hash64>) -> Result<KernelTokenizerV1, String> {
    let digest = palw_tokenizer_raw_commitment_v1(bytes);
    let commitment = commitment.filter(|c| *c != Hash64::default());
    if let Some(c) = commitment {
        if c != digest {
            return Err(format!("the tokenizer file hashes to {digest}, not the class's tokenizer commitment {c}"));
        }
    }
    let tokenizer = QwenTokenizer::from_json(bytes).map_err(|e| e.to_string())?;
    Ok(KernelTokenizerV1 { tokenizer, digest, verified: commitment.is_some() })
}

/// The job `text` asks of `class`: greedy, exactly `max_new` new tokens, `nonce` the requester's.
pub fn kernel_job_from_text_v1(
    tokenizer: &QwenTokenizer,
    text: &str,
    class: Hash64,
    token_bound: u32,
    max_positions: u32,
    max_new: u32,
    nonce: Hash64,
) -> Result<KernelJobV1, String> {
    let prompt = tokenizer.encode_without_specials(text).map_err(|e| e.to_string())?;
    let job = KernelJobV1 {
        class_binding_id: class.as_bytes(),
        prompt,
        max_new_tokens: max_new,
        decode: DecodeRuleV1::Greedy,
        nonce: nonce.as_bytes(),
    };
    job.well_formed(token_bound, max_positions).map_err(|why| format!("the class refuses this job: {why}"))?;
    Ok(job)
}

/// The text of `tokens` (an incomplete trailing UTF-8 sequence is held back, never substituted).
pub fn kernel_result_text_v1(tokenizer: &QwenTokenizer, tokens: &[u32]) -> Result<String, String> {
    match tokenizer.decode(tokens) {
        Ok(text) => Ok(text),
        Err(_) if tokens.iter().all(|t| tokenizer.token_bytes(*t).is_some()) => Ok(tokenizer.decode_lossy_tail(tokens)),
        Err(e) => Err(e.to_string()),
    }
}

/// Test support (also used by the node E2E): see [`tests::tiny_tokenizer_json`].
#[doc(hidden)]
pub mod tests {
    use super::*;

    /// A byte-level tokenizer of 32 ids (`a`..`z`, `Ġ` for space, `0`..`4`), no merges — the 32-token fixture class's whole range.
    pub fn tiny_tokenizer_json() -> Vec<u8> {
        let mut vocab = serde_json::Map::new();
        for (i, c) in ('a'..='z').chain(['Ġ']).chain('0'..='4').enumerate() {
            vocab.insert(c.to_string(), (i as u64).into());
        }
        serde_json::json!({ "model": { "type": "BPE", "vocab": vocab, "merges": [] }, "added_tokens": [] }).to_string().into_bytes()
    }

    #[cfg(test)]
    #[test]
    fn text_round_trips_through_a_job_and_back() {
        let bytes = tiny_tokenizer_json();
        let t = kernel_tokenizer_v1(&bytes, None).unwrap();
        assert!(!t.verified, "no commitment: used as given, never claimed verified");
        let job = kernel_job_from_text_v1(&t.tokenizer, "hi there", Hash64::from_bytes([7; 64]), 32, 64, 3, Hash64::default()).unwrap();
        assert_eq!(job.prompt.len(), 8);
        assert_eq!(kernel_result_text_v1(&t.tokenizer, &job.prompt).unwrap(), "hi there");
        assert!(kernel_job_from_text_v1(&t.tokenizer, "hi", Hash64::default(), 5, 64, 1, Hash64::default()).is_err(), "token bound");
        let good = kernel_tokenizer_v1(&bytes, Some(t.digest)).unwrap();
        assert!(good.verified);
        assert!(kernel_tokenizer_v1(&bytes, Some(Hash64::from_bytes([1; 64]))).is_err(), "a commitment the file misses refuses");
    }
}
