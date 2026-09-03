//! **The free-prompt seat, as ADR-0077 Decision 8 redefines it** — and the authentication
//! ADR-0077 SA-2 / ADR-0079 SA-3 put in front of the server that answers it.
//!
//! # What changed
//!
//! Until this module a free-prompt seat fetched the WHOLE capture (`request_palw_material`),
//! hashed all of it through `verify_material`, and sampled `k` leaves out of it (ADR-0073 Phase ①
//! 1e). That is bytes proportional to `decode_tokens_executed`: a 300-token answer on the hybrid
//! is gigabytes retained and megabytes moved per seat, and the number grows with exactly the thing
//! the lane exists to make bigger. Decision 8 replaces it on this lane:
//!
//! 1. The seat draws `k` checkpoint intervals from the claim's BEACON and its own seat index
//!    ([`palw_fp_interval_draw_v1`]) — unpredictable when the commitment was fixed, different per
//!    seat, and a pure function of chain facts anyone can recompute.
//! 2. The interval COUNT it draws inside comes from CHAIN data — the job's prompt length and the
//!    commitment's executed decode count — never from the executor. An executor that could shrink
//!    the count could predict the draw, and a draw the accused can predict is not a sample.
//! 3. It asks the executor for each interval's opening over the new P2P lane, signing the request
//!    with the bond's own ML-DSA-87 key.
//! 4. It replays each opening with the class's own kernels through the backend's
//!    `verify_fp_interval_opening` and compares EVERY ROW EXACTLY. The class is a pinned integer
//!    computation, so "close" is not a verdict.
//!
//! # What a seat's verdict is, and is not (W10)
//!
//! Every row equal: the seat files `Valid`. A row unequal: the seat files NOTHING and the fault is
//! the court's question — **a sampled verdict never slashes**. Nothing served: the two-sided
//! quorum's `ProducerDefaulted` arm, exactly as capture withholding reaches it today. This is
//! Ambient's "verify a window" shape without its proof model (ADR-0026): the licence a seat grants
//! keeps the claim disputable for the whole challenge window, and conviction runs only through the
//! court's bisection to one leaf.
//!
//! # Logging (ADR-0077 SA-5, ADR-0079 SA-7)
//!
//! **A seat logs no prompt ids and no prompt text.** "Private unless disputed" is false if the
//! default log is a disclosure, and it is false on the PUBLIC lane too — a node's log is not the
//! chain, and a prompt that is public on chain is still not this node's to republish in a form
//! nobody consented to. Claim ids, interval indices and leaf indices are public chain facts and
//! are logged; everything derived from the prompt is counted, never printed. A test in this file
//! reads the file back and holds the rule.

use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwFpIntervalVerdictV1};
use kaspa_consensus_core::palw_fp_interval_v1::{PALW_FP_SEAT_INTERVAL_SAMPLES_V1, palw_fp_interval_draw_v1};
use kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2;
use kaspa_hashes::Hash64;

// ---------------------------------------------------------------------------------------------
// SA-2: the request a bonded requester signs
// ---------------------------------------------------------------------------------------------

/// **The signing context for a served-data request** (ADR-0077 SA-2).
///
/// A NEW context string, and that is the whole security argument for reusing the bond's key: the
/// bond's ML-DSA-87 key already signs seat receipts under
/// `PALW_RECEIPT_V2_MLDSA87_CONTEXT`, and ML-DSA's context is part of what is verified, so a
/// signature produced here can never be replayed as a receipt and a receipt can never be replayed
/// as a request. Sharing the KEY is deliberate — the thing being proven is "I am this bond", which
/// is exactly what the bond's registry key means — while sharing a context would have made two
/// different statements interchangeable.
pub const PALW_FP_OPENING_REQUEST_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/fp-v3/opening-request/mldsa87/v1";

/// The message domain hashed under [`PALW_FP_OPENING_REQUEST_MLDSA87_CONTEXT`].
const PALW_FP_OPENING_REQUEST_DOMAIN: &[u8] = b"misaka-palw/fp-v3/opening-request-message/v1";

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    Hash64::from_bytes(state.finalize().as_bytes().try_into().expect("64 bytes"))
}

/// **What a requester signs** — `H(domain ‖ network_domain ‖ claim ‖ what ‖ requested_daa)`.
///
/// Every field a server acts on is inside, and that is not decoration:
///
/// * `network_domain` — a signature taken on devnet is not a serving right on testnet-11.
/// * `claim` and `what` — one signature must not be replayable as a request for every interval of
///   every claim. `what` is tagged, so an interval request and a whole-capture pull are different
///   messages even when the claim is the same: without the tag a captured interval request would
///   be a 16 MiB pull.
/// * `requested_daa` — a signature that never expires is a permanent serving right, transferable
///   by whoever captures the packet. The server refuses one outside its own freshness window.
pub fn palw_fp_opening_request_message_v1(
    network_domain: Hash64,
    claim: Hash64,
    interval_index: Option<u32>,
    requested_daa: u64,
) -> Hash64 {
    let mut state = keyed(PALW_FP_OPENING_REQUEST_DOMAIN);
    state.update(network_domain.as_byte_slice());
    state.update(claim.as_byte_slice());
    match interval_index {
        Some(index) => {
            state.update(&[1u8]);
            state.update(&index.to_le_bytes());
        }
        None => {
            state.update(&[2u8]);
        }
    }
    state.update(&requested_daa.to_le_bytes());
    finish(state)
}

/// Sign one request with the bond's key. `None` when the signer refuses, which a caller treats as
/// "this node cannot ask" — never as "ask unsigned".
pub fn palw_fp_sign_opening_request_v1(
    signing_key: &libcrux_ml_dsa::ml_dsa_87::MLDSA87SigningKey,
    network_domain: Hash64,
    claim: Hash64,
    interval_index: Option<u32>,
    requested_daa: u64,
) -> Option<Vec<u8>> {
    let message = palw_fp_opening_request_message_v1(network_domain, claim, interval_index, requested_daa);
    libcrux_ml_dsa::ml_dsa_87::sign(signing_key, message.as_byte_slice(), PALW_FP_OPENING_REQUEST_MLDSA87_CONTEXT, [0u8; 32])
        .ok()
        .map(|sig| sig.as_ref().to_vec())
}

/// Verify one request's signature under the requester's registry key. The bond lookup is the
/// caller's — this answers only "did the holder of this key sign THIS request".
pub fn palw_fp_verify_opening_request_v1(
    pubkey: &[u8],
    signature: &[u8],
    network_domain: Hash64,
    claim: Hash64,
    interval_index: Option<u32>,
    requested_daa: u64,
) -> bool {
    let message = palw_fp_opening_request_message_v1(network_domain, claim, interval_index, requested_daa);
    kaspa_txscript::verify_mldsa87_with_context(pubkey, message.as_byte_slice(), signature, PALW_FP_OPENING_REQUEST_MLDSA87_CONTEXT)
        .unwrap_or(false)
}

/// **The key the transport rate-limits a bond under** (ADR-0077 SA-2's "rate-limited per bond").
///
/// A hash of the bond's outpoint rather than the outpoint itself, so that the transport crate can
/// key a map on a requester's identity without naming a consensus type — and so that the value in
/// that map discloses nothing about which UTXO it is. It is not a security boundary: the identity
/// was already established by the signature and the chain lookup that produced this outpoint.
pub fn palw_fp_bond_rate_key_v1(bond: &kaspa_consensus_core::tx::TransactionOutpoint) -> Hash64 {
    let mut state = keyed(b"misaka-palw/fp-v3/opening-rate-key/v1");
    state.update(bond.transaction_id.as_bytes().as_slice());
    state.update(&bond.index.to_le_bytes());
    finish(state)
}

// ---------------------------------------------------------------------------------------------
// Decision 8: the draw
// ---------------------------------------------------------------------------------------------

/// **The two counts the interval draw needs, and where they are allowed to come from.**
///
/// Both are on the accepted 0x4a payload: the job's `prompt_tokens` and the commitment's
/// `decode_tokens_executed`. They are CHAIN facts, and the type exists to make that a compile-time
/// statement rather than a comment — a seat that read them off the served capture would be letting
/// the accused choose the number of intervals, and therefore the odds that any given interval is
/// drawn. An executor that reports one interval is an executor whose single interval is always
/// checked and whose other million are never opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwFpChainCountsV1 {
    pub prompt_tokens: u32,
    pub decode_tokens_executed: u32,
}

/// What the seat must open for one duty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwFpSeatDrawV1 {
    /// The intervals, in draw order. At most [`PALW_FP_SEAT_INTERVAL_SAMPLES_V1`], distinct, and
    /// every one below `interval_count`. A job with no more intervals than `k` is checked WHOLE —
    /// a short job costs the same as it used to and is simply verified in full.
    pub intervals: Vec<u32>,
    pub interval_count: u32,
}

/// **The draw for this seat, on this claim** (ADR-0077 Decision 8).
///
/// `None` when the class has no free-prompt path or the counts do not yield an interval count:
/// the seat then has no interval duty and falls back to the lane's other arms rather than
/// inventing one.
pub fn palw_fp_seat_draw_v1(
    backend: &dyn PalwExecutionBackendV1,
    network_domain: Hash64,
    duty: &PalwSeatDutyV2,
    counts: PalwFpChainCountsV1,
) -> Option<PalwFpSeatDrawV1> {
    let interval_count = backend.fp_interval_count_for(counts.prompt_tokens, counts.decode_tokens_executed)?;
    if interval_count == 0 {
        return None;
    }
    let intervals = palw_fp_interval_draw_v1(
        &network_domain,
        // The beacon: the block at the claim's anchor slot, which did not exist when the
        // commitment was fixed (ADR-0044 F4/F5). This is what the executor cannot predict.
        &duty.panel_anchor,
        &duty.claim_id,
        duty.seat_index,
        PALW_FP_SEAT_INTERVAL_SAMPLES_V1,
        interval_count,
    );
    Some(PalwFpSeatDrawV1 { intervals, interval_count })
}

// ---------------------------------------------------------------------------------------------
// W10: the bytes a seat fetches per claim, bounded
// ---------------------------------------------------------------------------------------------

/// **The bytes ONE opening may occupy** — `O(interval × row + log₂ leaves)`, ADR-0077 R1.
///
/// The three terms are the three things an opening carries: the checkpoint chunk and the committed
/// rows of the interval (`interval_positions × row_bytes`), the Merkle paths that bind them to the
/// claim's leg roots (`64 × ⌈log₂ leaves⌉`, one 64-byte digest per level), and a fixed header of
/// roots, indices and the interval's own ids.
///
/// **`step_leaf_count` enters only through a logarithm**, which is the whole of W10: doubling
/// `decode_tokens_executed` doubles the number of intervals and adds ONE digest to each opening's
/// path. It never widens an opening, because an opening is one interval and an interval is a class
/// constant.
pub fn palw_fp_interval_opening_ceiling_v1(interval_positions: u32, row_bytes: u32, step_leaf_count: u64) -> usize {
    /// Roots, indices, lengths and the interval's consumed/produced ids. Fixed, and generous:
    /// a ceiling that a real opening exceeds is a seat that refuses honest evidence, and this
    /// number costs nothing to over-state by a kilobyte.
    const OPENING_HEADER_BYTES: usize = 4 << 10;
    const DIGEST_BYTES: usize = 64;
    let depth = 64 - step_leaf_count.max(1).leading_zeros() as usize;
    (interval_positions as usize)
        .saturating_mul(row_bytes as usize)
        .saturating_add(DIGEST_BYTES.saturating_mul(depth))
        .saturating_add(OPENING_HEADER_BYTES)
}

/// **The bytes a seat fetches for one CLAIM** — `k` openings and nothing else. The whole-capture
/// pull is not in this number because Decision 8 retires it on this lane.
pub fn palw_fp_seat_claim_byte_ceiling_v1(interval_positions: u32, row_bytes: u32, step_leaf_count: u64) -> usize {
    (PALW_FP_SEAT_INTERVAL_SAMPLES_V1 as usize).saturating_mul(palw_fp_interval_opening_ceiling_v1(
        interval_positions,
        row_bytes,
        step_leaf_count,
    ))
}

// ---------------------------------------------------------------------------------------------
// Decision 8: the verdict
// ---------------------------------------------------------------------------------------------

/// What one seat concluded after replaying the intervals it drew.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwFpSeatOutcomeV1 {
    /// Every drawn interval replayed and every row matched. The seat may file `Valid`.
    Certified,
    /// A row did not match. **The seat files nothing** and opens a court at this leaf, as any
    /// bonded challenger may — it already holds the refutation's inputs. It does not accuse
    /// through a receipt, because a receipt is a quorum vote and a quorum is not a court.
    Fault { interval_index: u32, leaf_index: u64 },
    /// Nothing that binds to this claim arrived for at least one drawn interval. Not an
    /// accusation: the seat has simply not verified the claim, and the caller's existing tail
    /// (re-ask, then the half-window `Unavailable`) applies unchanged.
    NotVerified { unanswered: Vec<u32> },
}

/// Why a candidate opening was not the answer — counted, and named in the log without any part of
/// the prompt in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwFpOpeningRejectionV1 {
    /// Larger than this class's own ceiling. Refused BEFORE the backend is handed the bytes: the
    /// transport's cap is a backstop for every class at once, and this is the one the class's
    /// geometry actually implies.
    Oversized,
    /// It does not bind to this claim's roots — a forgery, or another claim's opening.
    Mismatch,
    /// Bytes this family cannot read.
    Unverifiable,
}

impl PalwFpOpeningRejectionV1 {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Oversized => "oversized",
            Self::Mismatch => "mismatch",
            Self::Unverifiable => "unverifiable",
        }
    }
}

/// **Replay the drawn intervals and compare every row exactly** (ADR-0077 Decision 8, W10).
///
/// `openings` holds every candidate payload the transport admitted for each drawn interval — up to
/// the lane's slot ceiling, because a forger who knows a live claim id and guesses a drawn index
/// can send one. Each is size-checked against `opening_ceiling` BEFORE it reaches the backend, and
/// a candidate that does not bind to the claim is skipped rather than believed: an interval is
/// answered if ANY candidate replays, and unanswered if none does.
///
/// A `Fault` short-circuits. It is the strongest thing anybody learns here and it is still not a
/// conviction — see [`PalwFpSeatOutcomeV1::Fault`].
pub fn palw_fp_seat_verify_openings_v1(
    backend: &dyn PalwExecutionBackendV1,
    roots: PalwClaimRootsV1,
    prompt_token_ids: &[u32],
    work_leaves: u64,
    draw: &PalwFpSeatDrawV1,
    openings: &dyn Fn(u32) -> Vec<Vec<u8>>,
    opening_ceiling: usize,
    mut on_rejection: impl FnMut(u32, PalwFpOpeningRejectionV1),
) -> PalwFpSeatOutcomeV1 {
    let mut unanswered = Vec::new();
    for index in &draw.intervals {
        let mut answered = false;
        for candidate in openings(*index) {
            if candidate.len() > opening_ceiling {
                on_rejection(*index, PalwFpOpeningRejectionV1::Oversized);
                continue;
            }
            match backend.verify_fp_interval_opening(&candidate, roots, *index, prompt_token_ids, work_leaves) {
                PalwFpIntervalVerdictV1::Valid => {
                    answered = true;
                    break;
                }
                PalwFpIntervalVerdictV1::Fault { leaf_index } => {
                    return PalwFpSeatOutcomeV1::Fault { interval_index: *index, leaf_index };
                }
                PalwFpIntervalVerdictV1::Mismatch => on_rejection(*index, PalwFpOpeningRejectionV1::Mismatch),
                PalwFpIntervalVerdictV1::Unverifiable => on_rejection(*index, PalwFpOpeningRejectionV1::Unverifiable),
            }
        }
        if !answered {
            unanswered.push(*index);
        }
    }
    if unanswered.is_empty() { PalwFpSeatOutcomeV1::Certified } else { PalwFpSeatOutcomeV1::NotVerified { unanswered } }
}

// ---------------------------------------------------------------------------------------------
// P-16 / Decision 16: PanelDa, the seat's half (W8)
// ---------------------------------------------------------------------------------------------

/// **What a seat does when a `PanelDa` claim's ids are missing or wrong** (ADR-0077 SA-5).
///
/// It files the panel's existing `Unavailable` arm and never `Valid`. `Unavailable` is not a
/// slashing verdict on its own — it is one side of the two-sided quorum, and with ADR-0065
/// Decision 4 armed a claim whose seats abstain reaches no quorum, is redrawn, and voids at
/// timeout WITHOUT a slash. That is the whole of SA-5: the mode's enforcement is a licence until
/// ADR-0062 lands, so a seat that cannot read the question declines to answer it rather than
/// convicting the producer of withholding.
///
/// The predicate itself is [`kaspa_consensus_core::palw_freeprompt_v3::palw_fp_seat_prompt_admit_v1`]
/// and is spelled ONCE, on the consensus side, because the chain, the court and the seat must
/// agree about what "the ids bind" means — a second spelling here is how two spellings come to
/// disagree.
pub const PALW_PANEL_DA_ABSTAINS_UNTIL_ADR_0062: bool = true;

/// **May this seat judge a claim in this privacy mode?** (Decision 16, and the fence it rides.)
///
/// `PublicDa` always: the ids are on the commitment. `PanelDa` only where the network carries the
/// rule — `Params::palw_panel_da_admissible()`, passed in rather than read here so the decision is
/// a pure function of two facts and testable without a preset.
///
/// The gate this replaces was `privacy_mode == PALW_FP_PRIVACY_PUBLIC_DA`, hard-coded in three
/// places, and it was not merely conservative: a seat that skips every mode-2 payload holds
/// nothing, files `Unavailable` at the half-window, and every honest mode-2 claim on a network
/// that armed the fence would void with its executor accused. Anything the chain admits, a seat
/// must be able to judge.
pub fn palw_fp_seat_may_judge_mode_v1(privacy_mode: u8, panel_da_admissible: bool) -> bool {
    use kaspa_consensus_core::palw_freeprompt_v3::{PALW_FP_PRIVACY_PANEL_DA, PALW_FP_PRIVACY_PUBLIC_DA};
    privacy_mode == PALW_FP_PRIVACY_PUBLIC_DA || (privacy_mode == PALW_FP_PRIVACY_PANEL_DA && panel_da_admissible)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
    use kaspa_consensus_core::tx::TransactionOutpoint;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn bond(v: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_bytes([v; 64]), 0))
    }

    fn duty(seat_index: u8, claim: u64, anchor: u64) -> PalwSeatDutyV2 {
        PalwSeatDutyV2 {
            accepted_block: h(1),
            claim_id: h(claim),
            class_id: h(2),
            artifact_root: h(3),
            seat_bond: bond(1),
            executor_bond: bond(2),
            execution_root: h(4),
            trace_root: h(5),
            bound_daa: 100,
            receipt_deadline: 1_300,
            panel_anchor: h(anchor),
            seat_index,
            pwu: 1_000,
            quanta: 1,
            free_prompt: true,
            work_leaves: 4_096,
        }
    }

    /// A backend that answers only the two verbs this module uses, so the seat's logic is under
    /// test rather than a family's arithmetic. `intervals_per` is the class's checkpoint geometry:
    /// one interval per `positions_per_interval` positions, which is what makes the count grow
    /// with the decode budget and the OPENING not.
    struct StubBackend {
        positions_per_interval: u32,
        /// `(index, opening bytes) -> verdict`, so a test can plant a fault, a mismatch or a
        /// tampered byte.
        verdicts: Mutex<BTreeMap<(u32, Vec<u8>), PalwFpIntervalVerdictV1>>,
        honest: Mutex<BTreeMap<u32, Vec<u8>>>,
        seen_work_leaves: Mutex<Vec<u64>>,
    }

    impl StubBackend {
        fn new(positions_per_interval: u32) -> Self {
            Self {
                positions_per_interval,
                verdicts: Mutex::new(BTreeMap::new()),
                honest: Mutex::new(BTreeMap::new()),
                seen_work_leaves: Mutex::new(Vec::new()),
            }
        }
        /// The executor's side: `open_fp_interval` for an interval this stub "ran".
        fn open(&self, index: u32, bytes: &[u8]) {
            self.honest.lock().unwrap().insert(index, bytes.to_vec());
            self.verdicts.lock().unwrap().insert((index, bytes.to_vec()), PalwFpIntervalVerdictV1::Valid);
        }
    }

    impl PalwExecutionBackendV1 for StubBackend {
        fn model_id(&self) -> &str {
            "stub"
        }
        fn job_for_anchor(&self, _anchor: Hash64) -> Result<(kaspa_consensus_core::palw_v2::PalwJobContextV2, Vec<usize>), String> {
            Err("not used".into())
        }
        fn execute(
            &self,
            _job: &kaspa_consensus_core::palw_v2::PalwJobContextV2,
            _prompt: &[usize],
        ) -> Result<kaspa_consensus_core::palw_backend::PalwExecutionOutcomeV1, String> {
            Err("not used".into())
        }
        fn verify_material(
            &self,
            _material: &[u8],
            _claim: PalwClaimRootsV1,
        ) -> kaspa_consensus_core::palw_backend::PalwMaterialVerdictV1 {
            kaspa_consensus_core::palw_backend::PalwMaterialVerdictV1::Unverifiable
        }
        /// Interval 0 is the prefill and the calls to the first checkpoint; interval `j ≥ 1` is
        /// the calls after checkpoint `j − 1`. The count therefore grows with the decode budget,
        /// which is exactly what W10 asserts the FETCHED BYTES do not.
        fn fp_interval_count_for(&self, prompt_tokens: u32, decode_tokens_executed: u32) -> Option<u32> {
            let positions = prompt_tokens as u64 + decode_tokens_executed as u64;
            Some((positions.div_ceil(self.positions_per_interval as u64)).max(1) as u32)
        }
        fn open_fp_interval(&self, _capture: &[u8], index: u32, _prompt_token_ids: &[u32]) -> Result<Vec<u8>, String> {
            self.honest.lock().unwrap().get(&index).cloned().ok_or_else(|| "no such interval".to_string())
        }
        fn verify_fp_interval_opening(
            &self,
            opening: &[u8],
            _claim: PalwClaimRootsV1,
            index: u32,
            _prompt_token_ids: &[u32],
            work_leaves: u64,
        ) -> PalwFpIntervalVerdictV1 {
            self.seen_work_leaves.lock().unwrap().push(work_leaves);
            self.verdicts.lock().unwrap().get(&(index, opening.to_vec())).copied().unwrap_or(PalwFpIntervalVerdictV1::Mismatch)
        }
    }

    fn roots() -> PalwClaimRootsV1 {
        PalwClaimRootsV1 { execution_root: h(4), trace_root: h(5), anchor: h(9) }
    }

    /// **The whole path: draw, ask, verify, file** — on a fixture claim whose executor answers
    /// from `open_fp_interval` (ADR-0077 P-08).
    #[test]
    fn the_draw_asks_the_executor_and_a_clean_replay_certifies() {
        let backend = StubBackend::new(32);
        let d = duty(2, 0x33, 0x22);
        let counts = PalwFpChainCountsV1 { prompt_tokens: 14, decode_tokens_executed: 300 };
        let draw = palw_fp_seat_draw_v1(&backend, h(0x11), &d, counts).expect("the class has a free-prompt path");
        assert_eq!(draw.interval_count, (14 + 300u32).div_ceil(32), "the count is chain arithmetic, not the executor's word");
        assert_eq!(draw.intervals.len(), PALW_FP_SEAT_INTERVAL_SAMPLES_V1 as usize);
        let mut sorted = draw.intervals.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), draw.intervals.len(), "distinct");

        // The executor opens exactly what was asked for, and nothing else.
        for index in &draw.intervals {
            backend.open(*index, format!("opening-{index}").as_bytes());
        }
        let served: BTreeMap<u32, Vec<Vec<u8>>> = draw
            .intervals
            .iter()
            .map(|i| (*i, vec![backend.open_fp_interval(b"", *i, &[]).expect("the executor opens it")]))
            .collect();

        let outcome = palw_fp_seat_verify_openings_v1(
            &backend,
            roots(),
            &[1, 2, 3],
            d.work_leaves,
            &draw,
            &|i| served.get(&i).cloned().unwrap_or_default(),
            1 << 20,
            |_, _| panic!("nothing should be rejected"),
        );
        assert_eq!(outcome, PalwFpSeatOutcomeV1::Certified);
        // The claim's PRICE is passed to every replay: an opening that is honestly some other,
        // smaller job's is not this claim's evidence (ADR-0074 Decision 5).
        assert!(
            backend.seen_work_leaves.lock().unwrap().iter().all(|leaves| *leaves == d.work_leaves),
            "the seat prices every opening at what the chain priced the claim"
        );
    }

    /// **The draw is the beacon's and the seat's, not the executor's.** Two seats of one panel open
    /// different intervals, and the same seat on a different beacon opens different ones — which is
    /// what makes a sample a sample.
    #[test]
    fn two_seats_of_one_panel_draw_different_intervals() {
        let backend = StubBackend::new(1);
        let counts = PalwFpChainCountsV1 { prompt_tokens: 8, decode_tokens_executed: 4_000 };
        let seat_0 = palw_fp_seat_draw_v1(&backend, h(0x11), &duty(0, 0x33, 0x22), counts).unwrap();
        let seat_3 = palw_fp_seat_draw_v1(&backend, h(0x11), &duty(3, 0x33, 0x22), counts).unwrap();
        let other_beacon = palw_fp_seat_draw_v1(&backend, h(0x11), &duty(0, 0x33, 0x77), counts).unwrap();
        assert_ne!(seat_0.intervals, seat_3.intervals, "seat index moves the draw");
        assert_ne!(seat_0.intervals, other_beacon.intervals, "the beacon moves the draw");
        assert_eq!(seat_0, palw_fp_seat_draw_v1(&backend, h(0x11), &duty(0, 0x33, 0x22), counts).unwrap(), "and it is pure");
    }

    /// **A short job is checked whole.** With no more intervals than `k`, every interval is drawn:
    /// there is nothing to sample, and a lane that sampled anyway would leave part of a small
    /// claim unchecked for no saving at all.
    #[test]
    fn a_job_with_no_more_intervals_than_k_is_verified_in_full() {
        let backend = StubBackend::new(32);
        let draw = palw_fp_seat_draw_v1(
            &backend,
            h(0x11),
            &duty(1, 0x33, 0x22),
            PalwFpChainCountsV1 { prompt_tokens: 14, decode_tokens_executed: 2 },
        )
        .unwrap();
        assert_eq!(draw.interval_count, 1);
        assert_eq!(draw.intervals, vec![0]);
    }

    /// **An opening tampered by one byte is `Mismatch`, and a seat that gets one files nothing.**
    ///
    /// Not `Unavailable` — that is a signed accusation of withholding and this producer served
    /// something — and above all not a slash: the seat has simply not verified the claim, and the
    /// caller's re-ask and half-window tail apply unchanged.
    #[test]
    fn an_opening_tampered_by_one_byte_is_not_verified_and_convicts_nobody() {
        let backend = StubBackend::new(32);
        let d = duty(2, 0x33, 0x22);
        let draw = palw_fp_seat_draw_v1(&backend, h(0x11), &d, PalwFpChainCountsV1 { prompt_tokens: 14, decode_tokens_executed: 300 })
            .unwrap();
        for index in &draw.intervals {
            backend.open(*index, format!("opening-{index}").as_bytes());
        }
        let tampered_at = draw.intervals[1];
        let served: BTreeMap<u32, Vec<Vec<u8>>> = draw
            .intervals
            .iter()
            .map(|i| {
                let mut bytes = backend.open_fp_interval(b"", *i, &[]).unwrap();
                if *i == tampered_at {
                    bytes[0] ^= 0x01; // one byte
                }
                (*i, vec![bytes])
            })
            .collect();

        let mut rejections = Vec::new();
        let outcome = palw_fp_seat_verify_openings_v1(
            &backend,
            roots(),
            &[1, 2, 3],
            d.work_leaves,
            &draw,
            &|i| served.get(&i).cloned().unwrap_or_default(),
            1 << 20,
            |index, why| rejections.push((index, why)),
        );
        assert_eq!(outcome, PalwFpSeatOutcomeV1::NotVerified { unanswered: vec![tampered_at] });
        assert_eq!(rejections, vec![(tampered_at, PalwFpOpeningRejectionV1::Mismatch)]);
        assert!(
            !matches!(outcome, PalwFpSeatOutcomeV1::Fault { .. }),
            "a byte that does not bind is a forgery, not a fault — nothing here reaches the court"
        );
    }

    /// **A row that replays UNEQUAL is a `Fault`, and a fault is the court's question.** The seat
    /// returns the leaf and files no receipt: a receipt is a quorum vote, and a quorum is not a
    /// court (ADR-0028, W10's last clause).
    #[test]
    fn an_unequal_row_is_a_fault_the_seat_does_not_vote_on() {
        let backend = StubBackend::new(32);
        let d = duty(2, 0x33, 0x22);
        let draw = palw_fp_seat_draw_v1(&backend, h(0x11), &d, PalwFpChainCountsV1 { prompt_tokens: 14, decode_tokens_executed: 300 })
            .unwrap();
        for index in &draw.intervals {
            backend.open(*index, format!("opening-{index}").as_bytes());
        }
        let faulted = draw.intervals[0];
        backend
            .verdicts
            .lock()
            .unwrap()
            .insert((faulted, format!("opening-{faulted}").into_bytes()), PalwFpIntervalVerdictV1::Fault { leaf_index: 77 });
        let served: BTreeMap<u32, Vec<Vec<u8>>> =
            draw.intervals.iter().map(|i| (*i, vec![format!("opening-{i}").into_bytes()])).collect();
        let outcome = palw_fp_seat_verify_openings_v1(
            &backend,
            roots(),
            &[1, 2, 3],
            d.work_leaves,
            &draw,
            &|i| served.get(&i).cloned().unwrap_or_default(),
            1 << 20,
            |_, _| {},
        );
        assert_eq!(outcome, PalwFpSeatOutcomeV1::Fault { interval_index: faulted, leaf_index: 77 });
    }

    /// **An oversized candidate never reaches the backend** — the size is checked before the bytes
    /// are handed to any decoder, which is what "bounded before deserialising" means. A forger's
    /// slot is spent and the honest opening in the same slot still verifies.
    #[test]
    fn an_oversized_candidate_is_refused_before_the_backend_sees_it() {
        let backend = StubBackend::new(32);
        let d = duty(2, 0x33, 0x22);
        let draw = palw_fp_seat_draw_v1(&backend, h(0x11), &d, PalwFpChainCountsV1 { prompt_tokens: 14, decode_tokens_executed: 60 })
            .unwrap();
        for index in &draw.intervals {
            backend.open(*index, format!("opening-{index}").as_bytes());
        }
        let target = draw.intervals[0];
        let served: BTreeMap<u32, Vec<Vec<u8>>> = draw
            .intervals
            .iter()
            .map(|i| {
                let honest = format!("opening-{i}").into_bytes();
                if *i == target { (*i, vec![vec![0u8; 4_096], honest]) } else { (*i, vec![honest]) }
            })
            .collect();
        let mut rejections = Vec::new();
        let outcome = palw_fp_seat_verify_openings_v1(
            &backend,
            roots(),
            &[1, 2, 3],
            d.work_leaves,
            &draw,
            &|i| served.get(&i).cloned().unwrap_or_default(),
            64,
            |index, why| rejections.push((index, why)),
        );
        assert_eq!(outcome, PalwFpSeatOutcomeV1::Certified, "the honest opening in the same slot still answered");
        assert_eq!(rejections, vec![(target, PalwFpOpeningRejectionV1::Oversized)]);
        assert_eq!(
            backend.seen_work_leaves.lock().unwrap().len(),
            draw.intervals.len(),
            "the oversized candidate was never handed to the family"
        );
    }

    /// **W10, measured**: the bytes a seat fetches per claim are bounded independent of
    /// `decode_tokens_executed`.
    ///
    /// The three decode counts below are 1, 100 and 10,000 tokens on one class geometry (a 32-position
    /// checkpoint interval, a 2 KiB row). The interval COUNT grows by four orders of magnitude; the
    /// per-opening ceiling grows by the Merkle depth alone — one 64-byte digest per doubling — and
    /// the seat's per-claim budget is `k` of those. A pre-Decision-8 seat fetched the capture, which
    /// is the product of the two.
    #[test]
    fn the_bytes_a_seat_fetches_do_not_grow_with_the_decode_count() {
        const POSITIONS_PER_INTERVAL: u32 = 32;
        const ROW_BYTES: u32 = 2 << 10;
        let backend = StubBackend::new(POSITIONS_PER_INTERVAL);
        let mut measured = Vec::new();
        for decode in [1u32, 100, 10_000] {
            let counts = PalwFpChainCountsV1 { prompt_tokens: 14, decode_tokens_executed: decode };
            let draw = palw_fp_seat_draw_v1(&backend, h(0x11), &duty(2, 0x33, 0x22), counts).unwrap();
            // The leaf count is the whole run's; the OPENING's path depth is its logarithm.
            let leaves = (14 + decode as u64) * 64;
            let per_claim = palw_fp_seat_claim_byte_ceiling_v1(POSITIONS_PER_INTERVAL, ROW_BYTES, leaves);
            measured.push((decode, draw.interval_count, draw.intervals.len(), per_claim, leaves));
        }
        let (_, count_1, drawn_1, bytes_1, _) = measured[0];
        let (_, count_10k, drawn_10k, bytes_10k, _) = measured[2];

        assert_eq!(count_1, 1, "one interval at a one-token answer");
        assert!(count_10k >= 313, "the interval count DOES grow with the job: {count_10k}");
        assert_eq!(drawn_1, 1, "a one-interval job is checked whole");
        assert_eq!(drawn_10k, PALW_FP_SEAT_INTERVAL_SAMPLES_V1 as usize, "a long job is still four intervals");

        // The measured numbers, so a change to the formula shows up as a number and not as a
        // silently different bound: 4 × (32 × 2048 + 64 × depth + 4096).
        assert_eq!(bytes_1, 4 * (32 * 2048 + 64 * 10 + 4096), "decode = 1");
        assert_eq!(measured[1].3, 4 * (32 * 2048 + 64 * 13 + 4096), "decode = 100");
        assert_eq!(bytes_10k, 4 * (32 * 2048 + 64 * 20 + 4096), "decode = 10,000");
        assert!(
            bytes_10k - bytes_1 == 4 * 64 * 10,
            "ten thousand times the job costs ten more digests per opening, and nothing else: {bytes_1} -> {bytes_10k}"
        );
        // And the ceiling is a ceiling: it never approaches the whole capture, which at these
        // counts is `leaves × row`.
        let capture_bytes = (14u64 + 10_000) * 64 * ROW_BYTES as u64;
        assert!(
            (bytes_10k as u64) * 100 < capture_bytes,
            "the seat fetches less than a hundredth of the capture at 10k tokens: {bytes_10k} vs {capture_bytes}"
        );
    }

    /// **W8: a seat holding no ids cannot file `Valid`, and a hash mismatch is refused by name.**
    ///
    /// The predicate is the consensus side's — one spelling for the chain, the court and the seat.
    /// What is checked here is the SEAT's use of it: which refusals exist, that they are named,
    /// and that neither of them is a verdict the seat may file `Valid` on.
    #[test]
    fn panel_da_ids_are_checked_before_anything_else_is_read() {
        use kaspa_consensus_core::palw_freeprompt_v3::{
            PALW_FP_PRIVACY_PANEL_DA, PALW_FP_PRIVACY_PUBLIC_DA, PalwFpV3Error, palw_fp_seat_prompt_admit_v1,
        };
        let job = kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3 {
            version: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V3_VERSION,
            network_domain: h(1),
            class_id: h(2),
            executor_bond: bond(2).0,
            executor_pubkey: vec![0xAA; 8],
            operator_id: h(3),
            anchor_block: h(4),
            anchor_daa: 100,
            job_nonce: [5u8; 32],
            tokenizer_id: h(6),
            prompt_token_ids_hash: kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(&[7u32, 8, 9]),
            prompt_tokens: 3,
            decode_token_limit: 2,
            max_context_tokens: 16,
            privacy_mode: PALW_FP_PRIVACY_PANEL_DA,
            prompt_mode: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER,
            sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
            temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        };

        // Nothing served: the producer's default arm, and never `Valid`.
        assert_eq!(palw_fp_seat_prompt_admit_v1(&job, None), Err(PalwFpV3Error::PromptIdsUnavailable));
        // Served, but not this claim's: a different accusation with a different name.
        assert_eq!(palw_fp_seat_prompt_admit_v1(&job, Some(&[7u32, 8, 10])), Err(PalwFpV3Error::PromptIdsHashMismatch));
        assert_eq!(
            palw_fp_seat_prompt_admit_v1(&job, Some(&[7u32, 8])),
            Err(PalwFpV3Error::PromptIdsCountMismatch { got: 2, declared: 3 })
        );
        assert_eq!(palw_fp_seat_prompt_admit_v1(&job, Some(&[7u32, 8, 9])), Ok(()));

        // The mode gate, and the fence it rides: `PublicDa` always, `PanelDa` only where the
        // network carries the rule. The old hard-coded gate is the `false` column.
        assert!(palw_fp_seat_may_judge_mode_v1(PALW_FP_PRIVACY_PUBLIC_DA, false));
        assert!(palw_fp_seat_may_judge_mode_v1(PALW_FP_PRIVACY_PUBLIC_DA, true));
        assert!(!palw_fp_seat_may_judge_mode_v1(PALW_FP_PRIVACY_PANEL_DA, false), "unfenced, mode 2 is not admitted at all");
        assert!(palw_fp_seat_may_judge_mode_v1(PALW_FP_PRIVACY_PANEL_DA, true), "and where it IS admitted, a seat must judge it");
        assert!(!palw_fp_seat_may_judge_mode_v1(3, true), "a mode this build does not execute is a mode no seat judges");

        // SA-5: withholding is an ABSTENTION, not an accusation, until ADR-0062 lands. The flag is
        // what a future ADR flips, and this assertion is what makes flipping it deliberate.
        assert!(
            PALW_PANEL_DA_ABSTAINS_UNTIL_ADR_0062,
            "ADR-0077 SA-5: with ADR-0065 D4 armed, PanelDa withholding voids by timeout without a slash"
        );
    }

    /// **The request a seat signs binds the network, the claim, WHAT was asked for and when.**
    ///
    /// The `what` tag is the one that is easy to leave out and expensive to leave out: without it
    /// a captured 7 KB interval request would be a valid whole-capture pull, and the 16 MiB serve
    /// SA-2 exists to bound would be reachable with a replayed signature.
    #[test]
    fn the_signed_request_binds_every_field_a_server_acts_on() {
        let base = palw_fp_opening_request_message_v1(h(1), h(2), Some(3), 400);
        assert_ne!(base, palw_fp_opening_request_message_v1(h(9), h(2), Some(3), 400), "network");
        assert_ne!(base, palw_fp_opening_request_message_v1(h(1), h(9), Some(3), 400), "claim");
        assert_ne!(base, palw_fp_opening_request_message_v1(h(1), h(2), Some(4), 400), "interval");
        assert_ne!(base, palw_fp_opening_request_message_v1(h(1), h(2), None, 400), "the whole-capture pull is another message");
        assert_ne!(base, palw_fp_opening_request_message_v1(h(1), h(2), Some(3), 401), "daa");
        assert_eq!(base, palw_fp_opening_request_message_v1(h(1), h(2), Some(3), 400), "pure");
    }

    /// **The bond's key signs both a receipt and a request, and neither signature is the other.**
    /// The separation is the ML-DSA context, which is verified rather than declared: a signature
    /// produced under one context fails under the other, so a captured receipt is not a serving
    /// right and a captured request is not a vote.
    #[test]
    fn a_request_signature_verifies_only_under_its_own_context() {
        use kaspa_consensus_core::palw_panel_v2::PALW_RECEIPT_V2_MLDSA87_CONTEXT;
        assert_ne!(PALW_FP_OPENING_REQUEST_MLDSA87_CONTEXT, PALW_RECEIPT_V2_MLDSA87_CONTEXT);

        let keypair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([7u8; 32]);
        let signature = palw_fp_sign_opening_request_v1(&keypair.signing_key, h(1), h(2), Some(3), 400).expect("signs");
        let pubkey = keypair.verification_key.as_ref().to_vec();

        assert!(palw_fp_verify_opening_request_v1(&pubkey, &signature, h(1), h(2), Some(3), 400));
        assert!(!palw_fp_verify_opening_request_v1(&pubkey, &signature, h(1), h(2), Some(4), 400), "another interval");
        assert!(!palw_fp_verify_opening_request_v1(&pubkey, &signature, h(1), h(2), None, 400), "the whole-capture pull");
        assert!(!palw_fp_verify_opening_request_v1(&pubkey, &signature, h(1), h(9), Some(3), 400), "another claim");
        assert!(!palw_fp_verify_opening_request_v1(&pubkey, &signature, h(1), h(2), Some(3), 401), "another daa");

        let stranger = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([8u8; 32]);
        assert!(
            !palw_fp_verify_opening_request_v1(stranger.verification_key.as_ref(), &signature, h(1), h(2), Some(3), 400),
            "a stranger's key does not verify this signature"
        );

        // The same message under the receipt context must not verify — that is the domain
        // separation, checked rather than asserted in a comment.
        let message = palw_fp_opening_request_message_v1(h(1), h(2), Some(3), 400);
        assert!(
            !kaspa_txscript::verify_mldsa87_with_context(
                &pubkey,
                message.as_byte_slice(),
                &signature,
                PALW_RECEIPT_V2_MLDSA87_CONTEXT
            )
            .unwrap_or(false),
            "a request signature must not read as a receipt signature"
        );
    }

    /// **ADR-0077 SA-5 / ADR-0079 SA-7: a seat logs no prompt ids and no prompt text.**
    ///
    /// Read off the seat's own sources, because the rule is about what the shipped binary PRINTS
    /// and no runtime assertion can see that. Counts, claim ids, interval indices and leaf indices
    /// are public chain facts and are allowed; anything whose name says it carries the prompt is
    /// not. Both of the seat's files are scanned: "private unless disputed" is false if the
    /// default log is a disclosure, and a rule enforced in one file of two is not enforced.
    #[test]
    fn nothing_in_this_module_logs_a_prompt() {
        let seat = include_str!("palw_fp_seat.rs");
        let panel = include_str!("palw_panel.rs");
        let bodies =
            [("palw_fp_seat.rs", seat.split("fn nothing_in_this_module_logs_a_prompt").next().unwrap()), ("palw_panel.rs", panel)];
        for (file, body) in bodies {
            for (number, line) in body.lines().enumerate() {
                let logs = ["info!", "warn!", "trace!", "debug!", "error!", "println!", "eprintln!"];
                if !logs.iter().any(|macro_name| line.contains(macro_name)) {
                    continue;
                }
                for banned in ["prompt_token_ids", "prompt_ids", "prompt_text", "{prompt", "{ids", "ids:?", "prompt:?"] {
                    assert!(!line.contains(banned), "{file}:{} logs a prompt ({banned}): {line}", number + 1);
                }
            }
        }
    }
}
