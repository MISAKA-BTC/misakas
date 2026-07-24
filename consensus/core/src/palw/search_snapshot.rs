//! Canonical `SearchSnapshotV1` — node-anchored web-search snapshots (DA object version 3).
//!
//! Per ADR node-anchored-web-search-da: live search results are never a worker's free-form
//! external input. The node's retrieval service performs the search once, converts the outcome
//! into this canonical, versioned snapshot, and anchors it in the DA layer. Workers consume the
//! identical snapshot bytes; validators recompute the digest and the DA-01 chunk-tree root.
//!
//! Encoding rules (fail-closed): fixed field order, little-endian integers, `u64`
//! length-prefixed UTF-8 byte strings, one-byte enum tags. Unknown versions, out-of-bound
//! fields, trailing bytes, rank gaps, digest mismatches, and incoherent outcomes are all
//! decode/validation errors — a snapshot either round-trips bit-exactly or it does not exist.
//!
//! The snapshot does not carry its own DA root (that would be circular); the scheduler binds
//! `snapshot digest + DA commitment + availability deadline` through
//! [`PalwSearchSnapshotAnchorV1::signing_hash`].

use kaspa_hashes::{Hash64, blake2b_512_keyed};
use thiserror::Error;

use super::da::{PALW_SEARCH_SNAPSHOT_DA_OBJECT_VERSION_V1, PalwDaError, PalwReceiptDaCommitmentV1, palw_receipt_da_commitment};

/// Inner schema version of the snapshot body (the DA object version is
/// [`PALW_SEARCH_SNAPSHOT_DA_OBJECT_VERSION_V1`]).
pub const PALW_SEARCH_SNAPSHOT_VERSION_V1: u16 = 1;
/// Digest domain for the canonical snapshot bytes.
pub const PALW_SEARCH_SNAPSHOT_DIGEST_DOMAIN: &[u8] = b"misaka-palw-search-snapshot-digest-v1";
/// Signing domain for the scheduler-side snapshot anchor.
pub const PALW_SEARCH_SNAPSHOT_ANCHOR_DOMAIN: &[u8] = b"misaka-palw-search-anchor-v1";

/// Default retention window for admitted snapshots, in DAA score units.
pub const PALW_SEARCH_SNAPSHOT_RETENTION_DAA: u64 = 604_800;
/// A snapshot's claimed retrieval DAA score may lead the admitting sink by at most this much.
pub const PALW_SEARCH_SNAPSHOT_MAX_FUTURE_DAA_SLACK: u64 = 600;

/// Bounded sizes. These are consensus constants: raising them is a versioned schema change.
pub const PALW_SEARCH_MAX_QUERY_BYTES: usize = 2048;
/// Maximum ruleset identifier length.
pub const PALW_SEARCH_MAX_RULESET_BYTES: usize = 128;
/// Maximum provider identifier length.
pub const PALW_SEARCH_MAX_PROVIDER_ID_BYTES: usize = 64;
/// Maximum region tag length.
pub const PALW_SEARCH_MAX_REGION_BYTES: usize = 32;
/// Maximum language tag length.
pub const PALW_SEARCH_MAX_LANGUAGE_BYTES: usize = 32;
/// Maximum ranked results in one snapshot.
pub const PALW_SEARCH_MAX_RESULTS: usize = 16;
/// Maximum title length per result.
pub const PALW_SEARCH_MAX_TITLE_BYTES: usize = 1024;
/// Maximum URL length per result.
pub const PALW_SEARCH_MAX_URL_BYTES: usize = 2048;
/// Maximum snippet length per result.
pub const PALW_SEARCH_MAX_SNIPPET_BYTES: usize = 4096;
/// Maximum fetched-body records in one snapshot.
pub const PALW_SEARCH_MAX_BODIES: usize = 8;
/// Maximum content-type length per body record.
pub const PALW_SEARCH_MAX_CONTENT_TYPE_BYTES: usize = 128;

/// Snapshot-level typed outcome. Failures are first-class snapshots: "the search failed at
/// this time under this policy" is itself the anchored fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PalwSearchOutcomeV1 {
    /// Provider returned at least one usable ranked result.
    Ok,
    /// Provider answered with zero results.
    EmptyResults,
    /// Provider did not answer within the retrieval deadline.
    ProviderTimeout,
    /// Provider answered with a non-success HTTP status.
    ProviderHttpFailure {
        /// The HTTP status code observed.
        status: u16,
    },
    /// Provider answered but the payload was not decodable under the pinned policy.
    ProviderMalformed,
    /// The egress guard refused to contact the provider.
    EgressDenied,
}

impl PalwSearchOutcomeV1 {
    fn tag(self) -> u8 {
        match self {
            Self::Ok => 0,
            Self::EmptyResults => 1,
            Self::ProviderTimeout => 2,
            Self::ProviderHttpFailure { .. } => 3,
            Self::ProviderMalformed => 4,
            Self::EgressDenied => 5,
        }
    }

    fn allows_results(self) -> bool {
        matches!(self, Self::Ok)
    }
}

/// Media class of one ranked result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PalwSearchMediaTypeV1 {
    /// Ordinary web page.
    Web,
    /// News article.
    News,
    /// Image result.
    Image,
    /// Anything else the provider labels.
    Other,
}

impl PalwSearchMediaTypeV1 {
    fn tag(self) -> u8 {
        match self {
            Self::Web => 0,
            Self::News => 1,
            Self::Image => 2,
            Self::Other => 255,
        }
    }

    fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Web),
            1 => Some(Self::News),
            2 => Some(Self::Image),
            255 => Some(Self::Other),
            _ => None,
        }
    }
}

/// Per-body fetch status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PalwSearchBodyStatusV1 {
    /// Body fetched within every bound; `sha256`/`byte_len` cover the exact bytes.
    Ok,
    /// Body exceeded the size bound; nothing was hashed.
    Oversize,
    /// Fetch timed out; nothing was hashed.
    Timeout,
    /// Server answered with a non-success status; nothing was hashed.
    HttpFailure,
    /// The egress guard refused the target; nothing was hashed.
    EgressDenied,
    /// Content type was outside the pinned allowlist; nothing was hashed.
    ContentTypeRejected,
}

impl PalwSearchBodyStatusV1 {
    fn tag(self) -> u8 {
        match self {
            Self::Ok => 0,
            Self::Oversize => 1,
            Self::Timeout => 2,
            Self::HttpFailure => 3,
            Self::EgressDenied => 4,
            Self::ContentTypeRejected => 5,
        }
    }

    fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Ok),
            1 => Some(Self::Oversize),
            2 => Some(Self::Timeout),
            3 => Some(Self::HttpFailure),
            4 => Some(Self::EgressDenied),
            5 => Some(Self::ContentTypeRejected),
            _ => None,
        }
    }
}

/// Body payload kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PalwSearchBodyKindV1 {
    /// Raw response bytes.
    Raw,
    /// Pinned-extractor text projection of the response.
    ExtractedText,
}

impl PalwSearchBodyKindV1 {
    fn tag(self) -> u8 {
        match self {
            Self::Raw => 0,
            Self::ExtractedText => 1,
        }
    }

    fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Raw),
            1 => Some(Self::ExtractedText),
            _ => None,
        }
    }
}

/// Pinned provider policy under which the retrieval ran.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PalwSearchProviderPolicyV1 {
    /// Provider identifier (e.g. `searxng`).
    pub provider_id: String,
    /// Hash of the full provider policy document (allowlist, extractor, retention).
    pub policy_id: Hash64,
    /// Region setting sent to the provider.
    pub region: String,
    /// Language setting sent to the provider.
    pub language: String,
    /// Safe-search level (0 off, 1 moderate, 2 strict).
    pub safe_search: u8,
}

/// One ranked provider result.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PalwSearchResultV1 {
    /// 1-based provider rank; must be the dense sequence `1..=n`.
    pub rank: u16,
    /// Media class.
    pub media_type: PalwSearchMediaTypeV1,
    /// Result title as returned by the provider (bounded UTF-8).
    pub title: String,
    /// Result URL (`http://` or `https://` only).
    pub url: String,
    /// Provider snippet (bounded UTF-8).
    pub snippet: String,
}

/// Digest record of one fetched response body.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PalwSearchBodyRecordV1 {
    /// Rank of the result this body belongs to.
    pub result_rank: u16,
    /// Payload kind.
    pub kind: PalwSearchBodyKindV1,
    /// Fetch status; only [`PalwSearchBodyStatusV1::Ok`] carries meaningful bytes.
    pub status: PalwSearchBodyStatusV1,
    /// Response content type (bounded UTF-8; empty when unavailable).
    pub content_type: String,
    /// Exact byte length of the hashed payload (0 unless `status == Ok`).
    pub byte_len: u32,
    /// SHA-256 of the exact payload bytes (all-zero unless `status == Ok`).
    pub sha256: [u8; 32],
}

/// Canonical node-anchored search snapshot.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PalwSearchSnapshotV1 {
    /// Inner schema version; must be [`PALW_SEARCH_SNAPSHOT_VERSION_V1`].
    pub version: u16,
    /// Numeric network id (devnet/testnet suffix); bound at admission.
    pub network_id: u32,
    /// Genesis block hash of the anchoring network; bound at admission.
    pub genesis_hash: Hash64,
    /// Ruleset / schema pin string (e.g. `palw-search-v1`).
    pub ruleset_id: String,
    /// Scheduler assignment this snapshot serves. Zero is the unassigned-diagnostic
    /// sentinel; resolving a real assignment is a StopShip gate before P2P exposure.
    pub assignment_id: Hash64,
    /// Query exactly as submitted by the scheduler.
    pub original_query: String,
    /// Pinned v1 normalization of the original query (whitespace collapse + trim);
    /// [`normalize_query_v1`] is the only admissible derivation.
    pub normalized_query: String,
    /// SHA-256 of the original query bytes.
    pub original_query_sha256: [u8; 32],
    /// SHA-256 of the normalized query bytes.
    pub normalized_query_sha256: [u8; 32],
    /// Provider policy pin.
    pub provider: PalwSearchProviderPolicyV1,
    /// Node wall-clock at retrieval, Unix milliseconds.
    pub retrieval_unix_millis: u64,
    /// Selected-chain DAA score the node observed at retrieval.
    pub retrieval_daa_score: u64,
    /// Freshness deadline, Unix milliseconds; must be strictly after retrieval.
    pub freshness_deadline_millis: u64,
    /// Typed retrieval outcome.
    pub outcome: PalwSearchOutcomeV1,
    /// Ranked results (dense ranks `1..=n`; non-empty iff `outcome == Ok`).
    pub results: Vec<PalwSearchResultV1>,
    /// Fetched-body digest records; each must reference an existing rank.
    pub bodies: Vec<PalwSearchBodyRecordV1>,
}

/// Snapshot codec/validation failures. Every variant is fail-closed.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PalwSearchSnapshotError {
    /// Unknown DA object or snapshot version.
    #[error("unsupported search snapshot version {0}")]
    UnsupportedVersion(u16),
    /// Byte stream ended early, has trailing bytes, or a length prefix is inconsistent.
    #[error("search snapshot bytes are not canonical: {0}")]
    NonCanonical(&'static str),
    /// A field exceeds its pinned bound.
    #[error("search snapshot field {field} exceeds bound {bound}")]
    Bound {
        /// Which field.
        field: &'static str,
        /// The violated bound.
        bound: usize,
    },
    /// A text field is not valid UTF-8.
    #[error("search snapshot field {0} is not valid UTF-8")]
    Utf8(&'static str),
    /// Structural rule violated (rank sequence, outcome coherence, digest mismatch, ...).
    #[error("search snapshot is invalid: {0}")]
    Invalid(&'static str),
    /// DA layer rejected the canonical bytes.
    #[error("search snapshot DA commitment failed: {0}")]
    Da(#[from] PalwDaError),
}

/// The only admissible v1 query normalization: Unicode-preserving whitespace collapse + trim.
#[must_use]
pub fn normalize_query_v1(original: &str) -> String {
    original.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

fn push_var(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    out.extend_from_slice(bytes);
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], PalwSearchSnapshotError> {
        let end = self.offset.checked_add(len).ok_or(PalwSearchSnapshotError::NonCanonical("length overflow"))?;
        let piece = self.bytes.get(self.offset..end).ok_or(PalwSearchSnapshotError::NonCanonical("truncated"))?;
        self.offset = end;
        Ok(piece)
    }

    fn u8(&mut self) -> Result<u8, PalwSearchSnapshotError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, PalwSearchSnapshotError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().expect("two bytes")))
    }

    fn u32(&mut self) -> Result<u32, PalwSearchSnapshotError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("four bytes")))
    }

    fn u64(&mut self) -> Result<u64, PalwSearchSnapshotError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("eight bytes")))
    }

    fn hash(&mut self) -> Result<Hash64, PalwSearchSnapshotError> {
        Ok(Hash64::from_bytes(self.take(64)?.try_into().expect("sixty-four bytes")))
    }

    fn array32(&mut self) -> Result<[u8; 32], PalwSearchSnapshotError> {
        Ok(self.take(32)?.try_into().expect("thirty-two bytes"))
    }

    fn var(&mut self, field: &'static str, bound: usize) -> Result<&'a [u8], PalwSearchSnapshotError> {
        let len = self.u64()?;
        let len = usize::try_from(len).map_err(|_| PalwSearchSnapshotError::NonCanonical("length overflow"))?;
        if len > bound {
            return Err(PalwSearchSnapshotError::Bound { field, bound });
        }
        self.take(len)
    }

    fn text(&mut self, field: &'static str, bound: usize) -> Result<String, PalwSearchSnapshotError> {
        let bytes = self.var(field, bound)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| PalwSearchSnapshotError::Utf8(field))
    }
}

fn check_text(field: &'static str, value: &str, bound: usize) -> Result<(), PalwSearchSnapshotError> {
    if value.len() > bound {
        return Err(PalwSearchSnapshotError::Bound { field, bound });
    }
    Ok(())
}

impl PalwSearchSnapshotV1 {
    /// Structural validation shared by encode and decode. Bounds, dense ranks,
    /// outcome/result coherence, body references, URL schemes, query digests, and the
    /// pinned normalization are all enforced here.
    pub fn validate(&self) -> Result<(), PalwSearchSnapshotError> {
        if self.version != PALW_SEARCH_SNAPSHOT_VERSION_V1 {
            return Err(PalwSearchSnapshotError::UnsupportedVersion(self.version));
        }
        check_text("ruleset_id", &self.ruleset_id, PALW_SEARCH_MAX_RULESET_BYTES)?;
        if self.ruleset_id.is_empty() {
            return Err(PalwSearchSnapshotError::Invalid("ruleset_id must not be empty"));
        }
        check_text("original_query", &self.original_query, PALW_SEARCH_MAX_QUERY_BYTES)?;
        check_text("normalized_query", &self.normalized_query, PALW_SEARCH_MAX_QUERY_BYTES)?;
        if self.normalized_query.is_empty() {
            return Err(PalwSearchSnapshotError::Invalid("normalized_query must not be empty"));
        }
        if self.normalized_query != normalize_query_v1(&self.original_query) {
            return Err(PalwSearchSnapshotError::Invalid("normalized_query does not follow the pinned v1 normalization"));
        }
        if self.original_query_sha256 != sha256(self.original_query.as_bytes()) {
            return Err(PalwSearchSnapshotError::Invalid("original_query_sha256 mismatch"));
        }
        if self.normalized_query_sha256 != sha256(self.normalized_query.as_bytes()) {
            return Err(PalwSearchSnapshotError::Invalid("normalized_query_sha256 mismatch"));
        }
        check_text("provider_id", &self.provider.provider_id, PALW_SEARCH_MAX_PROVIDER_ID_BYTES)?;
        if self.provider.provider_id.is_empty() {
            return Err(PalwSearchSnapshotError::Invalid("provider_id must not be empty"));
        }
        check_text("region", &self.provider.region, PALW_SEARCH_MAX_REGION_BYTES)?;
        check_text("language", &self.provider.language, PALW_SEARCH_MAX_LANGUAGE_BYTES)?;
        if self.provider.safe_search > 2 {
            return Err(PalwSearchSnapshotError::Invalid("safe_search must be 0..=2"));
        }
        if self.freshness_deadline_millis <= self.retrieval_unix_millis {
            return Err(PalwSearchSnapshotError::Invalid("freshness deadline must be after retrieval"));
        }
        if self.results.len() > PALW_SEARCH_MAX_RESULTS {
            return Err(PalwSearchSnapshotError::Bound { field: "results", bound: PALW_SEARCH_MAX_RESULTS });
        }
        if self.outcome.allows_results() {
            if self.results.is_empty() {
                return Err(PalwSearchSnapshotError::Invalid("outcome Ok requires at least one result"));
            }
        } else if !self.results.is_empty() {
            return Err(PalwSearchSnapshotError::Invalid("non-Ok outcome must carry zero results"));
        }
        for (index, result) in self.results.iter().enumerate() {
            let expected = u16::try_from(index + 1).map_err(|_| PalwSearchSnapshotError::Invalid("rank overflow"))?;
            if result.rank != expected {
                return Err(PalwSearchSnapshotError::Invalid("result ranks must be the dense sequence 1..=n"));
            }
            check_text("title", &result.title, PALW_SEARCH_MAX_TITLE_BYTES)?;
            check_text("url", &result.url, PALW_SEARCH_MAX_URL_BYTES)?;
            check_text("snippet", &result.snippet, PALW_SEARCH_MAX_SNIPPET_BYTES)?;
            if !(result.url.starts_with("https://") || result.url.starts_with("http://")) {
                return Err(PalwSearchSnapshotError::Invalid("result URL must be http(s)"));
            }
        }
        if self.bodies.len() > PALW_SEARCH_MAX_BODIES {
            return Err(PalwSearchSnapshotError::Bound { field: "bodies", bound: PALW_SEARCH_MAX_BODIES });
        }
        let mut seen = std::collections::BTreeSet::new();
        for body in &self.bodies {
            if body.result_rank == 0 || usize::from(body.result_rank) > self.results.len() {
                return Err(PalwSearchSnapshotError::Invalid("body record references a missing rank"));
            }
            if !seen.insert((body.result_rank, body.kind.tag())) {
                return Err(PalwSearchSnapshotError::Invalid("duplicate body record for (rank, kind)"));
            }
            check_text("content_type", &body.content_type, PALW_SEARCH_MAX_CONTENT_TYPE_BYTES)?;
            if body.status != PalwSearchBodyStatusV1::Ok && (body.byte_len != 0 || body.sha256 != [0_u8; 32]) {
                return Err(PalwSearchSnapshotError::Invalid("failed body records must carry zero length and digest"));
            }
        }
        Ok(())
    }

    /// Canonical DA object bytes: `[da_object_version u16][snapshot fields...]`.
    pub fn encode(&self) -> Result<Vec<u8>, PalwSearchSnapshotError> {
        self.validate()?;
        let mut out = Vec::with_capacity(512);
        out.extend_from_slice(&PALW_SEARCH_SNAPSHOT_DA_OBJECT_VERSION_V1.to_le_bytes());
        out.extend_from_slice(&self.version.to_le_bytes());
        out.extend_from_slice(&self.network_id.to_le_bytes());
        out.extend_from_slice(self.genesis_hash.as_byte_slice());
        push_var(&mut out, self.ruleset_id.as_bytes());
        out.extend_from_slice(self.assignment_id.as_byte_slice());
        push_var(&mut out, self.original_query.as_bytes());
        push_var(&mut out, self.normalized_query.as_bytes());
        out.extend_from_slice(&self.original_query_sha256);
        out.extend_from_slice(&self.normalized_query_sha256);
        push_var(&mut out, self.provider.provider_id.as_bytes());
        out.extend_from_slice(self.provider.policy_id.as_byte_slice());
        push_var(&mut out, self.provider.region.as_bytes());
        push_var(&mut out, self.provider.language.as_bytes());
        out.push(self.provider.safe_search);
        out.extend_from_slice(&self.retrieval_unix_millis.to_le_bytes());
        out.extend_from_slice(&self.retrieval_daa_score.to_le_bytes());
        out.extend_from_slice(&self.freshness_deadline_millis.to_le_bytes());
        out.push(self.outcome.tag());
        let status = match self.outcome {
            PalwSearchOutcomeV1::ProviderHttpFailure { status } => status,
            _ => 0,
        };
        out.extend_from_slice(&status.to_le_bytes());
        out.extend_from_slice(&(self.results.len() as u16).to_le_bytes());
        for result in &self.results {
            out.extend_from_slice(&result.rank.to_le_bytes());
            out.push(result.media_type.tag());
            push_var(&mut out, result.title.as_bytes());
            push_var(&mut out, result.url.as_bytes());
            push_var(&mut out, result.snippet.as_bytes());
        }
        out.extend_from_slice(&(self.bodies.len() as u16).to_le_bytes());
        for body in &self.bodies {
            out.extend_from_slice(&body.result_rank.to_le_bytes());
            out.push(body.kind.tag());
            out.push(body.status.tag());
            push_var(&mut out, body.content_type.as_bytes());
            out.extend_from_slice(&body.byte_len.to_le_bytes());
            out.extend_from_slice(&body.sha256);
        }
        Ok(out)
    }

    /// Strict decoder: unknown versions, bound violations, malformed tags, truncation and
    /// trailing bytes all fail; the result additionally passes [`Self::validate`].
    pub fn decode_strict(bytes: &[u8]) -> Result<Self, PalwSearchSnapshotError> {
        let mut cursor = Cursor { bytes, offset: 0 };
        let object_version = cursor.u16()?;
        if object_version != PALW_SEARCH_SNAPSHOT_DA_OBJECT_VERSION_V1 {
            return Err(PalwSearchSnapshotError::UnsupportedVersion(object_version));
        }
        let version = cursor.u16()?;
        if version != PALW_SEARCH_SNAPSHOT_VERSION_V1 {
            return Err(PalwSearchSnapshotError::UnsupportedVersion(version));
        }
        let network_id = cursor.u32()?;
        let genesis_hash = cursor.hash()?;
        let ruleset_id = cursor.text("ruleset_id", PALW_SEARCH_MAX_RULESET_BYTES)?;
        let assignment_id = cursor.hash()?;
        let original_query = cursor.text("original_query", PALW_SEARCH_MAX_QUERY_BYTES)?;
        let normalized_query = cursor.text("normalized_query", PALW_SEARCH_MAX_QUERY_BYTES)?;
        let original_query_sha256 = cursor.array32()?;
        let normalized_query_sha256 = cursor.array32()?;
        let provider_id = cursor.text("provider_id", PALW_SEARCH_MAX_PROVIDER_ID_BYTES)?;
        let policy_id = cursor.hash()?;
        let region = cursor.text("region", PALW_SEARCH_MAX_REGION_BYTES)?;
        let language = cursor.text("language", PALW_SEARCH_MAX_LANGUAGE_BYTES)?;
        let safe_search = cursor.u8()?;
        let retrieval_unix_millis = cursor.u64()?;
        let retrieval_daa_score = cursor.u64()?;
        let freshness_deadline_millis = cursor.u64()?;
        let outcome_tag = cursor.u8()?;
        let outcome_status = cursor.u16()?;
        let outcome = match (outcome_tag, outcome_status) {
            (0, 0) => PalwSearchOutcomeV1::Ok,
            (1, 0) => PalwSearchOutcomeV1::EmptyResults,
            (2, 0) => PalwSearchOutcomeV1::ProviderTimeout,
            (3, status) => PalwSearchOutcomeV1::ProviderHttpFailure { status },
            (4, 0) => PalwSearchOutcomeV1::ProviderMalformed,
            (5, 0) => PalwSearchOutcomeV1::EgressDenied,
            _ => return Err(PalwSearchSnapshotError::NonCanonical("unknown or non-canonical outcome encoding")),
        };
        let result_count = usize::from(cursor.u16()?);
        if result_count > PALW_SEARCH_MAX_RESULTS {
            return Err(PalwSearchSnapshotError::Bound { field: "results", bound: PALW_SEARCH_MAX_RESULTS });
        }
        let mut results = Vec::with_capacity(result_count);
        for _ in 0..result_count {
            let rank = cursor.u16()?;
            let media_type = PalwSearchMediaTypeV1::from_tag(cursor.u8()?)
                .ok_or(PalwSearchSnapshotError::NonCanonical("unknown media type tag"))?;
            let title = cursor.text("title", PALW_SEARCH_MAX_TITLE_BYTES)?;
            let url = cursor.text("url", PALW_SEARCH_MAX_URL_BYTES)?;
            let snippet = cursor.text("snippet", PALW_SEARCH_MAX_SNIPPET_BYTES)?;
            results.push(PalwSearchResultV1 { rank, media_type, title, url, snippet });
        }
        let body_count = usize::from(cursor.u16()?);
        if body_count > PALW_SEARCH_MAX_BODIES {
            return Err(PalwSearchSnapshotError::Bound { field: "bodies", bound: PALW_SEARCH_MAX_BODIES });
        }
        let mut bodies = Vec::with_capacity(body_count);
        for _ in 0..body_count {
            let result_rank = cursor.u16()?;
            let kind = PalwSearchBodyKindV1::from_tag(cursor.u8()?)
                .ok_or(PalwSearchSnapshotError::NonCanonical("unknown body kind tag"))?;
            let status = PalwSearchBodyStatusV1::from_tag(cursor.u8()?)
                .ok_or(PalwSearchSnapshotError::NonCanonical("unknown body status tag"))?;
            let content_type = cursor.text("content_type", PALW_SEARCH_MAX_CONTENT_TYPE_BYTES)?;
            let byte_len = cursor.u32()?;
            let sha256 = cursor.array32()?;
            bodies.push(PalwSearchBodyRecordV1 { result_rank, kind, status, content_type, byte_len, sha256 });
        }
        if cursor.offset != bytes.len() {
            return Err(PalwSearchSnapshotError::NonCanonical("trailing bytes"));
        }
        let snapshot = Self {
            version,
            network_id,
            genesis_hash,
            ruleset_id,
            assignment_id,
            original_query,
            normalized_query,
            original_query_sha256,
            normalized_query_sha256,
            provider: PalwSearchProviderPolicyV1 { provider_id, policy_id, region, language, safe_search },
            retrieval_unix_millis,
            retrieval_daa_score,
            freshness_deadline_millis,
            outcome,
            results,
            bodies,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    /// Keyed BLAKE2b-512 digest of the canonical bytes.
    pub fn digest(&self) -> Result<Hash64, PalwSearchSnapshotError> {
        Ok(blake2b_512_keyed(PALW_SEARCH_SNAPSHOT_DIGEST_DOMAIN, &self.encode()?))
    }

    /// DA-01 chunk-tree commitment over the canonical bytes (object version 3).
    pub fn da_commitment(&self) -> Result<PalwReceiptDaCommitmentV1, PalwSearchSnapshotError> {
        Ok(palw_receipt_da_commitment(PALW_SEARCH_SNAPSHOT_DA_OBJECT_VERSION_V1, &self.encode()?)?)
    }
}

/// Scheduler-side anchor binding one admitted snapshot into a future JobSpec/assignment.
/// The scheduler signs [`Self::signing_hash`]; nothing here is inside the snapshot bytes,
/// so the binding is non-circular.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PalwSearchSnapshotAnchorV1 {
    /// Canonical snapshot digest.
    pub snapshot_digest: Hash64,
    /// DA object root of the canonical bytes.
    pub object_root: Hash64,
    /// Canonical byte length.
    pub object_len: u32,
    /// DA chunk count.
    pub chunk_count: u16,
    /// DAA score until which the node guarantees availability.
    pub availability_deadline_daa_score: u64,
}

impl PalwSearchSnapshotAnchorV1 {
    /// Domain-separated signing hash for the scheduler's assignment signature.
    #[must_use]
    pub fn signing_hash(&self) -> Hash64 {
        let mut preimage = Vec::with_capacity(64 + 64 + 4 + 2 + 8);
        preimage.extend_from_slice(self.snapshot_digest.as_byte_slice());
        preimage.extend_from_slice(self.object_root.as_byte_slice());
        preimage.extend_from_slice(&self.object_len.to_le_bytes());
        preimage.extend_from_slice(&self.chunk_count.to_le_bytes());
        preimage.extend_from_slice(&self.availability_deadline_daa_score.to_le_bytes());
        blake2b_512_keyed(PALW_SEARCH_SNAPSHOT_ANCHOR_DOMAIN, &preimage)
    }
}

/// Result of one successful node admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PalwSearchSnapshotAdmittedV1 {
    /// DA object root (durable store key).
    pub object_root: Hash64,
    /// Canonical snapshot digest.
    pub snapshot_digest: Hash64,
    /// Canonical byte length.
    pub object_len: u32,
    /// DA chunk count.
    pub chunk_count: u16,
    /// Sink DAA score at admission.
    pub admitted_daa_score: u64,
    /// DAA score after which the snapshot may be garbage-collected.
    pub retention_until_daa_score: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> PalwSearchSnapshotV1 {
        let original_query = "量子コンピュータ  とは".to_string();
        let normalized_query = normalize_query_v1(&original_query);
        PalwSearchSnapshotV1 {
            version: PALW_SEARCH_SNAPSHOT_VERSION_V1,
            network_id: 111,
            genesis_hash: Hash64::from_bytes([7; 64]),
            ruleset_id: "palw-search-v1".into(),
            assignment_id: Hash64::from_bytes([0; 64]),
            original_query_sha256: super::sha256(original_query.as_bytes()),
            normalized_query_sha256: super::sha256(normalized_query.as_bytes()),
            original_query,
            normalized_query,
            provider: PalwSearchProviderPolicyV1 {
                provider_id: "searxng".into(),
                policy_id: Hash64::from_bytes([9; 64]),
                region: "jp".into(),
                language: "ja-JP".into(),
                safe_search: 1,
            },
            retrieval_unix_millis: 1_784_800_000_000,
            retrieval_daa_score: 12_345,
            freshness_deadline_millis: 1_784_800_600_000,
            outcome: PalwSearchOutcomeV1::Ok,
            results: vec![
                PalwSearchResultV1 {
                    rank: 1,
                    media_type: PalwSearchMediaTypeV1::Web,
                    title: "量子コンピュータ - Wikipedia".into(),
                    url: "https://ja.wikipedia.org/wiki/%E9%87%8F%E5%AD%90".into(),
                    snippet: "重ね合わせ \"quote\" と\nもつれ".into(),
                },
                PalwSearchResultV1 {
                    rank: 2,
                    media_type: PalwSearchMediaTypeV1::News,
                    title: "news".into(),
                    url: "http://example.com/a".into(),
                    snippet: String::new(),
                },
            ],
            bodies: vec![PalwSearchBodyRecordV1 {
                result_rank: 1,
                kind: PalwSearchBodyKindV1::Raw,
                status: PalwSearchBodyStatusV1::Ok,
                content_type: "text/html; charset=utf-8".into(),
                byte_len: 4096,
                sha256: [3; 32],
            }],
        }
    }

    #[test]
    fn round_trips_bit_exactly() {
        let snapshot = snapshot();
        let bytes = snapshot.encode().unwrap();
        let decoded = PalwSearchSnapshotV1::decode_strict(&bytes).unwrap();
        assert_eq!(decoded, snapshot);
        assert_eq!(decoded.encode().unwrap(), bytes);
    }

    #[test]
    fn digest_and_commitment_are_stable_and_bound_to_every_byte() {
        let snapshot = snapshot();
        let bytes = snapshot.encode().unwrap();
        let digest = snapshot.digest().unwrap();
        let commitment = snapshot.da_commitment().unwrap();
        assert_eq!(commitment.object_version, PALW_SEARCH_SNAPSHOT_DA_OBJECT_VERSION_V1);
        assert_eq!(commitment.object_len as usize, bytes.len());
        // Any single-byte mutation must change the digest or fail decoding entirely.
        let step = (bytes.len() / 24).max(1);
        for index in (0..bytes.len()).step_by(step) {
            let mut mutated = bytes.clone();
            mutated[index] ^= 0x01;
            match PalwSearchSnapshotV1::decode_strict(&mutated) {
                Err(_) => {}
                Ok(decoded) => {
                    assert_ne!(decoded.digest().unwrap(), digest, "mutation at byte {index} kept the digest");
                }
            }
        }
    }

    #[test]
    fn every_truncation_fails_closed() {
        let bytes = snapshot().encode().unwrap();
        for len in 0..bytes.len() {
            assert!(PalwSearchSnapshotV1::decode_strict(&bytes[..len]).is_err(), "truncation to {len} decoded");
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(matches!(PalwSearchSnapshotV1::decode_strict(&trailing), Err(PalwSearchSnapshotError::NonCanonical(_))));
    }

    #[test]
    fn unknown_versions_fail_closed() {
        let bytes = snapshot().encode().unwrap();
        let mut wrong_object = bytes.clone();
        wrong_object[0] = 9;
        assert!(matches!(
            PalwSearchSnapshotV1::decode_strict(&wrong_object),
            Err(PalwSearchSnapshotError::UnsupportedVersion(9))
        ));
        let mut wrong_inner = bytes;
        wrong_inner[2] = 2;
        assert!(matches!(
            PalwSearchSnapshotV1::decode_strict(&wrong_inner),
            Err(PalwSearchSnapshotError::UnsupportedVersion(2))
        ));
    }

    #[test]
    fn structural_rules_fail_closed() {
        let mut gap = snapshot();
        gap.results[1].rank = 3;
        assert!(gap.encode().is_err());

        let mut incoherent = snapshot();
        incoherent.outcome = PalwSearchOutcomeV1::EmptyResults;
        assert!(incoherent.encode().is_err());

        let mut orphan_body = snapshot();
        orphan_body.bodies[0].result_rank = 5;
        assert!(orphan_body.encode().is_err());

        let mut failed_body_with_digest = snapshot();
        failed_body_with_digest.bodies[0].status = PalwSearchBodyStatusV1::Timeout;
        assert!(failed_body_with_digest.encode().is_err());

        let mut bad_normalization = snapshot();
        bad_normalization.normalized_query = "改変".into();
        assert!(bad_normalization.encode().is_err());

        let mut bad_query_digest = snapshot();
        bad_query_digest.original_query_sha256[0] ^= 1;
        assert!(bad_query_digest.encode().is_err());

        let mut bad_scheme = snapshot();
        bad_scheme.results[0].url = "ftp://example.com/x".into();
        assert!(bad_scheme.encode().is_err());

        let mut oversized = snapshot();
        oversized.results[0].snippet = "x".repeat(PALW_SEARCH_MAX_SNIPPET_BYTES + 1);
        assert!(matches!(oversized.encode(), Err(PalwSearchSnapshotError::Bound { field: "snippet", .. })));

        let mut deadline = snapshot();
        deadline.freshness_deadline_millis = deadline.retrieval_unix_millis;
        assert!(deadline.encode().is_err());
    }

    #[test]
    fn failure_outcomes_are_first_class_snapshots() {
        let mut failed = snapshot();
        failed.outcome = PalwSearchOutcomeV1::ProviderHttpFailure { status: 502 };
        failed.results.clear();
        failed.bodies.clear();
        let bytes = failed.encode().unwrap();
        let decoded = PalwSearchSnapshotV1::decode_strict(&bytes).unwrap();
        assert_eq!(decoded.outcome, PalwSearchOutcomeV1::ProviderHttpFailure { status: 502 });

        let mut timeout = failed;
        timeout.outcome = PalwSearchOutcomeV1::ProviderTimeout;
        let canonical = timeout.encode().unwrap();
        assert_eq!(PalwSearchSnapshotV1::decode_strict(&canonical).unwrap().outcome, PalwSearchOutcomeV1::ProviderTimeout);
        // A non-HttpFailure outcome with a nonzero status word is non-canonical: with empty
        // result/body lists the tail is [tag u8][status u16][results u16][bodies u16].
        let mut smuggled_status = canonical;
        let status_offset = smuggled_status.len() - 6;
        smuggled_status[status_offset] = 7;
        assert!(matches!(
            PalwSearchSnapshotV1::decode_strict(&smuggled_status),
            Err(PalwSearchSnapshotError::NonCanonical(_))
        ));
    }

    #[test]
    fn anchor_signing_hash_is_domain_separated_and_field_sensitive() {
        let snapshot = snapshot();
        let commitment = snapshot.da_commitment().unwrap();
        let anchor = PalwSearchSnapshotAnchorV1 {
            snapshot_digest: snapshot.digest().unwrap(),
            object_root: commitment.root,
            object_len: commitment.object_len,
            chunk_count: commitment.chunk_count,
            availability_deadline_daa_score: 99_999,
        };
        let base = anchor.signing_hash();
        let mut moved = anchor;
        moved.availability_deadline_daa_score += 1;
        assert_ne!(base, moved.signing_hash());
        assert_ne!(base.as_byte_slice(), snapshot.digest().unwrap().as_byte_slice());
    }

    #[test]
    fn normalization_pin_is_whitespace_collapse_and_trim() {
        assert_eq!(normalize_query_v1("  a\t\tb \n c  "), "a b c");
        assert_eq!(normalize_query_v1("量子  コンピュータ"), "量子 コンピュータ");
    }
}
