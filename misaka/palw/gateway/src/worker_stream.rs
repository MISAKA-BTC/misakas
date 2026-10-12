//! **RFC-0001 P1 — a run's frames, read with a cancel.**
//!
//! A resident worker streams `Token` frames and then ONE terminator. When the client of the request leaves mid-run the gateway used to
//! drain the rest and discard it ("the resident worker has no cancel frame, and killing it would make every dropped connection cost a model
//! re-map"). The worker now reads a `Cancel` frame while it decodes and stops at its next token, answering `Cancelled`; this module is the
//! gateway's half: it polls the caller's `cancelled()` while it reads the tokens, writes the cancel frame ONCE the moment the client is
//! gone, and then reads on to the terminator — so the stream stays in step and the worker, its artifact and its KV cache stay resident for
//! the next request.
//!
//! * **One cancel, to the request in flight.** The frame names the request hash the gateway computed from its own bytes; a worker that
//!   never heard of it (an older build that refuses the frame as "not a v3 request") simply runs on, and the gateway drains and discards as
//!   before — a cancel is a request to save work, never a requirement.
//! * **The terminator decides.** `Cancelled` ends the run as [`CANCELLED_BY_CLIENT`]; a `Result` that arrives anyway (the cancel lost the
//!   race with the last token) is returned like any other and the caller discards it, exactly as before; a `Refused` is a refusal.
//! * **A cancel nobody sent is a broken stream.** A worker that answers `Cancelled` to a request that was not cancelled is not to be
//!   trusted with the next one: the error is not the cancellation, and the caller drops the worker.
//! * **Polling is bounded.** `cancelled()` can cost a syscall (a socket peek), so it is asked every [`CANCEL_POLL_TOKENS`] tokens.

use std::io::{Read, Write};

use kaspa_consensus_core::palw_freeprompt_v3::{
    PalwFpWorkerFrameV1, PalwFpWorkerRequestV3, PalwFpWorkerResultV3, fp_worker_cancel_frame_v1,
};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_v2::{PALW_V2_MAX_FRAME_BYTES, write_framed};
use kaspa_hashes::Hash64;

use crate::serving::CANCELLED_BY_CLIENT;
use crate::{AnswerRun, wire};

/// `cancelled()` is asked once per this many tokens.
pub const CANCEL_POLL_TOKENS: u32 = 4;

/// What was said to the worker mid-run, for the caller's accounting (and the tests').
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CancelState {
    pub tokens: u32,
    pub cancel_sent: bool,
}

/// Write the cancel frame for `request_hash` (flushed: the worker is blocked in a decode loop and reads it between tokens).
fn send_cancel<W: Write>(stdin: &mut W, request_hash: Hash64) -> Result<(), String> {
    write_framed(stdin, &fp_worker_cancel_frame_v1(request_hash)).map_err(|e| format!("cannot write the cancel frame: {e}"))?;
    stdin.flush().map_err(|e| format!("cannot flush the cancel frame: {e}"))
}

/// One token seen: count it, and at the poll interval ask whether the client is gone — sending the cancel once if so.
fn on_token_seen<W: Write>(
    stdin: &mut W,
    request_hash: Hash64,
    cancelled: &dyn Fn() -> bool,
    state: &mut CancelState,
) -> Result<(), String> {
    state.tokens += 1;
    if !state.cancel_sent && state.tokens.is_multiple_of(CANCEL_POLL_TOKENS) && cancelled() {
        send_cancel(stdin, request_hash)?;
        state.cancel_sent = true;
    }
    Ok(())
}

/// **Read a committed run's frames** (the request frame is already written): tokens to `on_token`, the cancel when `cancelled()` says the
/// client left, and the terminator — a `Result` re-bound to `request` (the worker is never trusted about what it was asked), `Refused`, or
/// `Cancelled`.
pub fn read_committed_run<R: Read, W: Write>(
    stdin: &mut W,
    stdout: &mut R,
    request: &PalwFpWorkerRequestV3,
    request_hash: Hash64,
    prompt_ids_form: PalwPromptIdsFormV1,
    on_token: &mut dyn FnMut(u32, &[u8]),
    cancelled: &dyn Fn() -> bool,
) -> Result<PalwFpWorkerResultV3, String> {
    let mut state = CancelState::default();
    loop {
        let Some(bytes) = wire::read_frame_stream(stdout, PALW_V2_MAX_FRAME_BYTES)? else {
            return Err("the worker stream ended before a terminator frame".to_string());
        };
        match borsh::from_slice::<PalwFpWorkerFrameV1>(&bytes).map_err(|e| format!("a worker frame does not decode: {e}"))? {
            PalwFpWorkerFrameV1::Token { token_id, rendered } => {
                on_token(token_id, &rendered);
                on_token_seen(stdin, request_hash, cancelled, &mut state)?;
            }
            PalwFpWorkerFrameV1::Result(result) => {
                // The caller-side re-binding: the worker is never trusted about what it was asked, and `request_hash` is re-derived from
                // OUR canonical encoding. (P2: the shared validator, F2 included; the manifest binding is the caller's, which holds it.)
                result
                    .validate_against_request(request, request_hash, prompt_ids_form)
                    .map_err(|e| format!("the worker result does not bind the request: {e}"))?;
                return Ok(*result);
            }
            PalwFpWorkerFrameV1::Refused { reason } => return Err(format!("the worker refused the job: {reason}")),
            PalwFpWorkerFrameV1::Cancelled { request_hash: named } => {
                return if state.cancel_sent && named == request_hash {
                    Err(CANCELLED_BY_CLIENT.to_string())
                } else {
                    Err("the worker cancelled a run nobody cancelled (or named another request): the stream is not to be trusted"
                        .to_string())
                };
            }
            PalwFpWorkerFrameV1::Manifest(_) => {
                return Err("the worker re-announced its manifest mid-session".to_string());
            }
            PalwFpWorkerFrameV1::Answered(_)
            | PalwFpWorkerFrameV1::AnsweredBatch(_)
            | PalwFpWorkerFrameV1::BatchToken { .. }
            | PalwFpWorkerFrameV1::Embedded(_) => {
                return Err("the worker answered a committed job with an answer-only frame".to_string());
            }
        }
    }
}

/// **Read an answer-only run's frames** (RFC-0001 §2.6): the same discipline, ending in `Answered`, `Refused` (`Unsupported` for a worker
/// that serves no answer-only path) or `Cancelled`.
pub fn read_answer_run<R: Read, W: Write>(
    stdin: &mut W,
    stdout: &mut R,
    request_hash: Hash64,
    on_token: &mut dyn FnMut(u32, &[u8]),
    cancelled: &dyn Fn() -> bool,
) -> Result<AnswerRun, String> {
    let mut state = CancelState::default();
    loop {
        let Some(bytes) = wire::read_frame_stream(stdout, PALW_V2_MAX_FRAME_BYTES)? else {
            return Err("the worker stream ended before a terminator frame".to_string());
        };
        match borsh::from_slice::<PalwFpWorkerFrameV1>(&bytes).map_err(|e| format!("a worker frame does not decode: {e}"))? {
            PalwFpWorkerFrameV1::Token { token_id, rendered } => {
                on_token(token_id, &rendered);
                on_token_seen(stdin, request_hash, cancelled, &mut state)?;
            }
            PalwFpWorkerFrameV1::Answered(answer) => {
                if answer.request_hash != request_hash {
                    return Err("the worker's answer does not bind the request it was asked".to_string());
                }
                return Ok(AnswerRun::Answered(*answer));
            }
            PalwFpWorkerFrameV1::Refused { reason } => {
                return if reason.contains("serves no answer-only path") || reason.contains("not a v3 request") {
                    Ok(AnswerRun::Unsupported)
                } else {
                    Err(format!("the worker refused the job: {reason}"))
                };
            }
            PalwFpWorkerFrameV1::Cancelled { request_hash: named } => {
                return if state.cancel_sent && named == request_hash {
                    Err(CANCELLED_BY_CLIENT.to_string())
                } else {
                    Err("the worker cancelled a run nobody cancelled (or named another request): the stream is not to be trusted"
                        .to_string())
                };
            }
            PalwFpWorkerFrameV1::Result(_)
            | PalwFpWorkerFrameV1::AnsweredBatch(_)
            | PalwFpWorkerFrameV1::BatchToken { .. }
            | PalwFpWorkerFrameV1::Embedded(_) => {
                return Err("the worker answered an answer-only request with a frame of another kind".to_string());
            }
            PalwFpWorkerFrameV1::Manifest(_) => {
                return Err("the worker re-announced its manifest mid-session".to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_freeprompt_v3::{PalwFpWorkerAnswerV1, parse_fp_worker_cancel_frame_v1};
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::rc::Rc;

    fn frame_bytes(frame: &PalwFpWorkerFrameV1) -> Vec<u8> {
        let payload = borsh::to_vec(frame).unwrap();
        let mut out = (payload.len() as u32).to_le_bytes().to_vec();
        out.extend_from_slice(&payload);
        out
    }

    /// What the gateway wrote to the worker's stdin, shared with the scripted worker so it can react to a cancel the way the real one does.
    #[derive(Clone, Default)]
    struct Wire(Rc<RefCell<Vec<u8>>>);

    impl Write for Wire {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Wire {
        /// The request hashes of the cancel frames written so far.
        fn cancels(&self) -> Vec<Hash64> {
            let bytes = self.0.borrow().clone();
            let mut cursor = bytes.as_slice();
            let mut out = Vec::new();
            while let Ok(Some(frame)) = wire::read_frame_stream(&mut cursor, PALW_V2_MAX_FRAME_BYTES) {
                out.extend(parse_fp_worker_cancel_frame_v1(&frame));
            }
            out
        }
    }

    /// A worker that decodes one token per read until it has said all `tokens`, then ends with `terminator`; once it has seen a cancel on
    /// its stdin it stops after the token it is on and answers `Cancelled` (the real worker's behaviour, to the token).
    struct ScriptedWorker {
        wire: Wire,
        queue: VecDeque<u8>,
        tokens_left: u32,
        terminator: PalwFpWorkerFrameV1,
        cancel_answer: Option<PalwFpWorkerFrameV1>,
        /// What follows the terminator: proof of where the stream stands (the next request's first frame).
        after: PalwFpWorkerFrameV1,
        sent_tokens: Rc<Cell<u32>>,
        finished: bool,
    }

    impl Read for ScriptedWorker {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.queue.is_empty() && !self.finished {
                let frame = if !self.wire.cancels().is_empty() && self.cancel_answer.is_some() {
                    self.finished = true;
                    self.cancel_answer.clone().unwrap()
                } else if self.tokens_left > 0 {
                    self.tokens_left -= 1;
                    self.sent_tokens.set(self.sent_tokens.get() + 1);
                    PalwFpWorkerFrameV1::Token { token_id: 7, rendered: b"x".to_vec() }
                } else {
                    self.finished = true;
                    self.terminator.clone()
                };
                self.queue.extend(frame_bytes(&frame));
                if self.finished {
                    self.queue.extend(frame_bytes(&self.after));
                }
            }
            let n = buf.len().min(self.queue.len());
            for slot in buf.iter_mut().take(n) {
                *slot = self.queue.pop_front().unwrap();
            }
            Ok(n)
        }
    }

    fn worker(
        wire: &Wire,
        tokens: u32,
        terminator: PalwFpWorkerFrameV1,
        cancel_answer: Option<PalwFpWorkerFrameV1>,
    ) -> (ScriptedWorker, Rc<Cell<u32>>) {
        let sent = Rc::new(Cell::new(0));
        let w = ScriptedWorker {
            wire: wire.clone(),
            queue: VecDeque::new(),
            tokens_left: tokens,
            terminator,
            cancel_answer,
            after: PalwFpWorkerFrameV1::Refused { reason: "the next request's frame".to_string() },
            sent_tokens: sent.clone(),
            finished: false,
        };
        (w, sent)
    }

    fn request() -> PalwFpWorkerRequestV3 {
        use kaspa_consensus_core::palw_freeprompt_v3::{
            PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFpWorkerInputV3,
        };
        PalwFpWorkerRequestV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: Hash64::from_u64_word(1),
            class_id: Hash64::from_u64_word(2),
            executor_bond: kaspa_consensus_core::tx::TransactionOutpoint::new(Hash64::from_u64_word(3), 0),
            executor_pubkey: vec![4; 8],
            operator_id: Hash64::from_u64_word(5),
            anchor_block: Hash64::from_u64_word(6),
            anchor_daa: 7,
            job_nonce: [8; 32],
            decode_token_limit: 64,
            max_context_tokens: 128,
            privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: PALW_FP_PROMPT_MODE_USER,
            sampling_seed: [0; 32],
            temperature_q: 0,
            input: PalwFpWorkerInputV3::TokenIds(vec![1, 2, 3]),
            model_profile_id: Hash64::from_u64_word(10),
            runtime_manifest_hash: Hash64::from_u64_word(11),
            runtime_class_id: Hash64::from_u64_word(12),
            shape_profile_id: Hash64::from_u64_word(13),
            trace_scheme_id: Hash64::from_u64_word(14),
            decode: None,
            stop_texts: Vec::new(),
            constraint: None,
        }
    }

    const FORM: PalwPromptIdsFormV1 = PalwPromptIdsFormV1::Flat;

    fn hash_of(r: &PalwFpWorkerRequestV3) -> Hash64 {
        kaspa_consensus_core::palw_freeprompt_v3::fp_worker_request_hash_v3(&borsh::to_vec(r).unwrap())
    }

    /// **The client leaves at token 5: one cancel frame, naming the request, goes to the worker at the next poll (token 8); the worker
    /// stops at the token it is on and answers `Cancelled`; the run ends as a cancellation with 8 tokens seen of 64 — and the stream stands
    /// exactly at the next frame, so the same worker serves the next request.**
    #[test]
    fn a_run_whose_client_left_sends_one_cancel_and_ends_as_a_cancellation_with_the_stream_in_step() {
        let r = request();
        let h = hash_of(&r);
        let stdin = Wire::default();
        let (mut stdout, sent) = worker(
            &stdin,
            64,
            PalwFpWorkerFrameV1::Refused { reason: "unreachable: the run was cancelled".to_string() },
            Some(PalwFpWorkerFrameV1::Cancelled { request_hash: h }),
        );
        let seen = Cell::new(0u32);
        let gone_after = 5;
        let outcome = read_committed_run(&mut stdin.clone(), &mut stdout, &r, h, FORM, &mut |_, _| seen.set(seen.get() + 1), &|| {
            seen.get() >= gone_after
        });
        assert_eq!(outcome, Err(CANCELLED_BY_CLIENT.to_string()));
        assert_eq!(stdin.cancels(), vec![h], "exactly one cancel, naming the request");
        // Polled at tokens 4 (client still here) and 8 (gone): the worker stopped on the token it was on, so it said 8 and no more.
        assert_eq!(seen.get(), 8);
        assert_eq!(sent.get(), 8, "the worker said no token after the cancel reached it");
        // The stream is in step: the next frame is the one after the terminator.
        let rest = wire::read_frame_stream(&mut stdout, PALW_V2_MAX_FRAME_BYTES).unwrap().expect("a frame follows");
        assert_eq!(
            borsh::from_slice::<PalwFpWorkerFrameV1>(&rest).unwrap(),
            PalwFpWorkerFrameV1::Refused { reason: "the next request's frame".to_string() }
        );
    }

    #[test]
    fn a_client_that_stays_costs_no_cancel_frame_and_the_refusal_is_the_workers() {
        let r = request();
        let h = hash_of(&r);
        let stdin = Wire::default();
        let (mut stdout, _) = worker(&stdin, 10, PalwFpWorkerFrameV1::Refused { reason: "no room".to_string() }, None);
        let polls = Cell::new(0u32);
        let outcome = read_committed_run(&mut stdin.clone(), &mut stdout, &r, h, FORM, &mut |_, _| {}, &|| {
            polls.set(polls.get() + 1);
            false
        });
        assert_eq!(outcome, Err("the worker refused the job: no room".to_string()));
        assert!(stdin.cancels().is_empty() && stdin.0.borrow().is_empty(), "nothing was written to the worker");
        assert_eq!(polls.get(), 2, "polled at tokens 4 and 8 of 10, not at every token");
    }

    #[test]
    fn a_worker_that_cancels_a_run_nobody_cancelled_is_a_broken_stream_not_a_cancellation() {
        let r = request();
        let h = hash_of(&r);
        let stdin = Wire::default();
        // Unprompted: the client never left.
        let (mut stdout, _) = worker(&stdin, 3, PalwFpWorkerFrameV1::Cancelled { request_hash: h }, None);
        let e = read_committed_run(&mut stdin.clone(), &mut stdout, &r, h, FORM, &mut |_, _| {}, &|| false).unwrap_err();
        assert!(e.contains("nobody cancelled") && !crate::serving::is_cancelled(&e), "{e}");
        // The client left, but the worker names ANOTHER request.
        let other = Hash64::from_u64_word(0xBAD);
        let (mut stdout, _) = worker(
            &stdin,
            64,
            PalwFpWorkerFrameV1::Refused { reason: String::new() },
            Some(PalwFpWorkerFrameV1::Cancelled { request_hash: other }),
        );
        let e = read_committed_run(&mut stdin.clone(), &mut stdout, &r, h, FORM, &mut |_, _| {}, &|| true).unwrap_err();
        assert!(e.contains("nobody cancelled") && !crate::serving::is_cancelled(&e), "{e}");
    }

    #[test]
    fn the_answer_only_path_cancels_the_same_way_and_a_stream_that_ends_early_is_an_error() {
        let r = request();
        let h = hash_of(&r);
        let stdin = Wire::default();
        let answer = PalwFpWorkerAnswerV1 {
            request_hash: h,
            prompt_token_ids: vec![1, 2, 3],
            output_token_ids: vec![7; 3],
            rendered: b"xxx".to_vec(),
            cached_prefix_tokens: 0,
            ended_on_stop_id: false,
            stop_sequence_len: None,
            execute_ms: 1,
        };
        // Finishes before the first poll: an answer.
        let (mut stdout, _) = worker(&stdin, 3, PalwFpWorkerFrameV1::Answered(Box::new(answer)), None);
        assert!(matches!(read_answer_run(&mut stdin.clone(), &mut stdout, h, &mut |_, _| {}, &|| true), Ok(AnswerRun::Answered(_))));
        assert!(stdin.cancels().is_empty(), "three tokens never reached the poll interval");
        // The client leaves: cancelled.
        let (mut stdout, sent) = worker(
            &stdin,
            64,
            PalwFpWorkerFrameV1::Refused { reason: String::new() },
            Some(PalwFpWorkerFrameV1::Cancelled { request_hash: h }),
        );
        let outcome = read_answer_run(&mut stdin.clone(), &mut stdout, h, &mut |_, _| {}, &|| true);
        assert_eq!(outcome.unwrap_err(), CANCELLED_BY_CLIENT);
        assert_eq!(stdin.cancels(), vec![h]);
        assert_eq!(sent.get(), 4, "cancelled at the first poll");
        // A worker that serves no answer-only path is `Unsupported`, as before.
        let (mut stdout, _) =
            worker(&stdin, 0, PalwFpWorkerFrameV1::Refused { reason: "this backend serves no answer-only path".to_string() }, None);
        assert!(matches!(read_answer_run(&mut stdin.clone(), &mut stdout, h, &mut |_, _| {}, &|| false), Ok(AnswerRun::Unsupported)));
        // The stream ending before a terminator is an error, never a silent success.
        let mut empty: &[u8] = &[];
        assert!(read_answer_run(&mut stdin.clone(), &mut empty, h, &mut |_, _| {}, &|| false).is_err());
    }
}
