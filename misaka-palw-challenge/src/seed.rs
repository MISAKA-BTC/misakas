//! **The one seed derivation and its domain-separated samplers** (RFC-0007 §§VI.5–VI.6).
//!
//! `challenge_seed = H(CHALLENGE; subject ‖ challenge_anchor ‖ beacon_output)`, defined only here, after the subject is committed
//! and the beacon is locked. Streams derive from it per `(kind, scope, relation, repetition)`; each is a BLAKE2b counter-mode
//! word stream with **rejection** sampling (no modulo bias) and a named exhaustion bound instead of a biased fallback.
//! Interactive proofs take each round's challenge either from a NEW locked beacon after that round's message was committed
//! ([`staged_round_challenge_v1`]) or from a transcript-bound transform absorbing every prior message and challenge
//! ([`FiatShamirTranscriptV1`]); publishing one seed and deriving every future round from it is never offered.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::beacon::{BeaconContextV1, WorkBeaconV1};
use crate::hash::{DOMAIN_CHALLENGE, DOMAIN_FS_ROUND, DOMAIN_STAGED_ROUND, DOMAIN_STREAM_BLOCK, DOMAIN_STREAM_KEY, Digest, object_id};
use crate::policy::InteractiveModeV1;
use crate::subject::ChallengeSubjectV1;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SeedRefusalV1 {
    #[error("the subject names another challenge policy than the beacon's")]
    PolicyMismatch,
    #[error("the subject's kind or commitment root is not the one the beacon was collected for")]
    SubjectMismatch,
    #[error("the subject's chain or ruleset is not the beacon's")]
    ChainMismatch,
}

/// **`challenge_seed`**, from a committed subject and a locked beacon of the same context.
pub fn challenge_seed_v1(ctx: &BeaconContextV1, subject: &ChallengeSubjectV1, beacon: &WorkBeaconV1) -> Result<Digest, SeedRefusalV1> {
    if subject.challenge_policy_id != ctx.policy.id() {
        return Err(SeedRefusalV1::PolicyMismatch);
    }
    if subject.subject_kind != ctx.subject_kind || subject.commitment_root != ctx.commitment_root {
        return Err(SeedRefusalV1::SubjectMismatch);
    }
    if subject.chain_genesis != ctx.chain_genesis || subject.ruleset_id != ctx.ruleset_id {
        return Err(SeedRefusalV1::ChainMismatch);
    }
    Ok(object_id(DOMAIN_CHALLENGE, &(subject, beacon.challenge_anchor, beacon.output)))
}

/// What a stream is for (distinct domains; RFC-0007 §VI.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum StreamKindV1 {
    Query = 1,
    Segment = 2,
    TensorRange = 3,
    Vector = 4,
    Freivalds = 5,
    Aggregation = 6,
    ProofRound = 7,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct StreamLabelV1 {
    pub kind: StreamKindV1,
    /// The scope (relation family, tensor, segment set) the stream serves.
    pub scope_id: Digest,
    pub relation: u32,
    pub repetition: u32,
}

/// The default bound on 64-bit words one stream may draw before it reports exhaustion.
pub const STREAM_MAX_WORDS_V1: u64 = 1 << 24;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SampleRefusalV1 {
    #[error("the stream exhausted its {0}-word bound")]
    Exhausted(u64),
    #[error("an empty range")]
    EmptyRange,
    #[error("asked {count} distinct indices of {n}")]
    TooMany { count: u64, n: u64 },
}

/// A deterministic word stream for one label of one seed.
#[derive(Clone, Debug)]
pub struct ChallengeStreamV1 {
    key: Digest,
    block: u64,
    buf: [u8; 64],
    used: usize,
    words: u64,
    max_words: u64,
}

/// The Mersenne prime `2^127 − 1`, the kernel's Freivalds field.
pub const P127: u128 = (1u128 << 127) - 1;

impl ChallengeStreamV1 {
    pub fn new(seed: &Digest, label: &StreamLabelV1) -> Self {
        Self::with_bound(seed, label, STREAM_MAX_WORDS_V1)
    }

    pub fn with_bound(seed: &Digest, label: &StreamLabelV1, max_words: u64) -> Self {
        let key = object_id(DOMAIN_STREAM_KEY, &(*seed, *label));
        Self { key, block: 0, buf: [0; 64], used: 64, words: 0, max_words }
    }

    pub fn words_drawn(&self) -> u64 {
        self.words
    }

    pub fn next_u64(&mut self) -> Result<u64, SampleRefusalV1> {
        if self.words >= self.max_words {
            return Err(SampleRefusalV1::Exhausted(self.max_words));
        }
        if self.used == 64 {
            self.buf = object_id(DOMAIN_STREAM_BLOCK, &(self.key, self.block));
            self.block += 1;
            self.used = 0;
        }
        let w = u64::from_le_bytes(self.buf[self.used..self.used + 8].try_into().expect("8 bytes"));
        self.used += 8;
        self.words += 1;
        Ok(w)
    }

    /// Uniform in `[0, n)` by rejection: words at or above the largest multiple of `n` are redrawn.
    pub fn index_below(&mut self, n: u64) -> Result<u64, SampleRefusalV1> {
        if n == 0 {
            return Err(SampleRefusalV1::EmptyRange);
        }
        // `zone` is the largest multiple of n not exceeding 2^64: accept w < zone (computed without overflow).
        let reject = (u64::MAX - n + 1) % n; // = 2^64 mod n
        loop {
            let w = self.next_u64()?;
            if reject == 0 || w < u64::MAX - reject + 1 {
                return Ok(w % n);
            }
        }
    }

    /// `count` distinct indices of `[0, n)`, uniformly without replacement (Floyd), returned sorted.
    pub fn distinct_indices(&mut self, n: u64, count: u64) -> Result<Vec<u64>, SampleRefusalV1> {
        if count > n {
            return Err(SampleRefusalV1::TooMany { count, n });
        }
        let mut set = std::collections::BTreeSet::new();
        for j in (n - count)..n {
            let t = self.index_below(j + 1)?;
            if !set.insert(t) {
                set.insert(j);
            }
        }
        Ok(set.into_iter().collect())
    }

    /// Uniform in `GF(2^127 − 1)`: 127 bits by rejection of the single value `p`.
    pub fn field_m127(&mut self) -> Result<u128, SampleRefusalV1> {
        loop {
            let lo = self.next_u64()? as u128;
            let hi = self.next_u64()? as u128;
            let x = ((hi << 64) | lo) & P127;
            if x != P127 {
                return Ok(x);
            }
        }
    }

    /// A vector of `len` field elements.
    pub fn field_vector_m127(&mut self, len: usize) -> Result<Vec<u128>, SampleRefusalV1> {
        (0..len).map(|_| self.field_m127()).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TranscriptRefusalV1 {
    #[error("the policy's interactive mode is {0:?}")]
    WrongMode(InteractiveModeV1),
    #[error("round {round}'s message was committed at {message_position}, not before its source window starts at {window_start}")]
    MessageAfterWindow { round: u32, message_position: u64, window_start: u64 },
    #[error("round {0} is out of order")]
    OutOfOrder(u32),
    #[error("round {0} does not match its recomputation")]
    Mismatch(u32),
}

/// **Staged beacon** round `round`: a fresh locked beacon whose source window began after the round's message was committed.
pub fn staged_round_challenge_v1(
    mode: InteractiveModeV1,
    base_seed: &Digest,
    round: u32,
    message_root: &Digest,
    message_position: u64,
    round_beacon: &WorkBeaconV1,
    round_window_start: u64,
) -> Result<Digest, TranscriptRefusalV1> {
    if mode != InteractiveModeV1::StagedBeacon {
        return Err(TranscriptRefusalV1::WrongMode(mode));
    }
    if message_position >= round_window_start {
        return Err(TranscriptRefusalV1::MessageAfterWindow { round, message_position, window_start: round_window_start });
    }
    Ok(object_id(DOMAIN_STAGED_ROUND, &(*base_seed, round, *message_root, round_beacon.challenge_anchor, round_beacon.output)))
}

/// **Transcript-bound Fiat–Shamir**: each challenge absorbs the statement, the policy/source binding, the round index and every
/// prior message and challenge, including this round's message. Its soundness needs its own review (an external gate).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FiatShamirTranscriptV1 {
    pub statement_root: Digest,
    pub challenge_policy_id: Digest,
    /// The locked beacon's output and anchor the transcript is bound to.
    pub source_binding: Digest,
    /// `(message_root, challenge)` per round, in order.
    pub rounds: Vec<(Digest, Digest)>,
}

impl FiatShamirTranscriptV1 {
    pub fn new(statement_root: Digest, challenge_policy_id: Digest, beacon: &WorkBeaconV1) -> Self {
        let source_binding = object_id(DOMAIN_FS_ROUND, &(u32::MAX, beacon.challenge_anchor, beacon.output));
        Self { statement_root, challenge_policy_id, source_binding, rounds: Vec::new() }
    }

    fn derive(&self, round: u32, message_root: &Digest) -> Digest {
        object_id(
            DOMAIN_FS_ROUND,
            &(
                self.statement_root,
                self.challenge_policy_id,
                self.source_binding,
                round,
                &self.rounds[..round as usize],
                *message_root,
            ),
        )
    }

    /// Absorb the next round's message and return its challenge.
    pub fn absorb(&mut self, mode: InteractiveModeV1, message_root: Digest) -> Result<Digest, TranscriptRefusalV1> {
        if mode != InteractiveModeV1::TranscriptBoundFiatShamir {
            return Err(TranscriptRefusalV1::WrongMode(mode));
        }
        let round = self.rounds.len() as u32;
        let c = self.derive(round, &message_root);
        self.rounds.push((message_root, c));
        Ok(c)
    }

    /// Recompute every round from canonical bytes.
    pub fn verify(&self) -> Result<(), TranscriptRefusalV1> {
        for (i, (m, c)) in self.rounds.iter().enumerate() {
            if self.derive(i as u32, m) != *c {
                return Err(TranscriptRefusalV1::Mismatch(i as u32));
            }
        }
        Ok(())
    }
}
