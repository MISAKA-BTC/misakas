//! **RFC-0059 & ADR-0227 (`PALW-AN1`) — Actual Input Trial Binding, Two-Freeze Schedule, Full Panel Replay, and Model-Free Node Local Court (with RFC-0056 §17, RFC-0057 §17, RFC-0058 §16 Adapters).**
//!
//! Status: **DRAFT_NOT_ACTIVATED** (`RFC-0059 §0`).
//!
//! This module implements the normative `PALW-AN1` architecture and its integration bridges with
//! `RFC-0056 §17` (`CAD1`), `RFC-0057 §17` (`CPN1`), and `RFC-0058 §16` (`SWC1`):
//!
//! 1. **Registration Context `C`, `RegKey`, `slot_id`, `attempt_id (s)`, `ComputationalWorkKey`, and `BaseWorkKey` (`RFC-0059 §2`, `RFC-0058 §16.2`)**:
//!    - One `RegKey = H("AN1-registration-key", Enc(C))` maps atomically to at most one `slot_id = H("AN1-slot", Enc(C))`.
//!      Re-packaging identical `C` under different funding, carrier, or signature cannot mint another slot (`AN1-01`, `A56-02`).
//!    - Post-registration execution beacon `sigma` derives `s = H("AN1-attempt", Enc(C), sigma)`.
//!    - Separates `ComputationalWorkKey` (bound to actual challenge-prefixed input tokens `x`) from
//!      `BaseWorkKey` (the underlying job demand before challenge prefixing) so purpose-specific
//!      useful-work credits (`AN1-TEST-USEFUL-ONCE-1`) cannot be minted twice by varying `nonce` or `owner` (`AN1-11`).
//!
//! 2. **Injective Byte-to-Token Mapping & Challenge-First Input (`RFC-0059 §3`, `RFC-0058 §16.3`)**:
//!    - `BOS = 0`, `POW = 1`, `SEP = 2`, `ByteToken[b] = 3 + b` (`b in 0..=255`, vocabulary `259`).
//!    - `x = [BOS, POW] || [ByteToken[s_0], ..., ByteToken[s_31]] || [SEP] || job_tokens` (`len = 35 + len(job_tokens)`).
//!    - Rejects out-of-vocabulary tokens, budget overflow (`> 8` job tokens in the reference profile),
//!      truncation, UNK collapsing, or challenge-dropping templates (`AN1-02`, `AN1-03`, `AN1-W02`).
//!
//! 3. **Two Freezes & Strict 257-Bit Target Threshold `theta(D)` (`RFC-0059 §4–§5`, `RFC-0056 §17.1–§17.2`)**:
//!    - Freeze 1: Registration `C` closes strictly before earliest access to execution beacon `sigma`.
//!    - Freeze 2: Signed original `Claim` (`s, R_x, R_y, R_tau`) is sealed once before `panel_event`.
//!    - `Ticket = uint256_be(H("AN1-ticket", s, R_x, R_y, R_tau))` checked against
//!      `theta(D) = floor((2^256 - 1) / D) + 1` in a 257-bit representation (`An1TargetThreshold257V1`)
//!      so `D = 1 => theta = 2^256` without `uint256` wraparound, matching `ticket <= T(D)` (`AN1-10`, `A56-03`).
//!    - Tie-break rank is `(ticket, slot_id)` (`RFC-0056 §17.2`).
//!
//! 4. **Context-Bound Padded Merkle Trees, Causal Program, Full Panel Replay & Model-Free Node Local Court (`RFC-0059 §6–§9`, `RFC-0057 §17`)**:
//!    - Every real leaf (`AN1-real-leaf`), padding leaf (`AN1-padding-leaf`), and internal node (`AN1-internal`)
//!      commits to `(kind, scope, n, L, coordinates)` with `L = 2^ceil(log2(n))` and `MAX_LEAF_BYTES = 512`.
//!    - Strict integer type checking (`An1CheckedScalarV1`) rejects floats (`6153.0`, `0.0`), booleans (`true`, `false`),
//!      strings, and out-of-range integers identically at native admission and in Court (`AN1-15`, `A56-09`, `A57-12`).
//!    - `an1_judge_court_v1` operates on model-free `An1CourtContextV1`, derives the expected weight index
//!      from the authenticated instruction and parent values, and separates `InvalidEvidence`, `NoRefutation`,
//!      and `ComputationRefuted` (`AN1-05`, `AN1-06`, `A57-03`..`A57-06`).
//!    - `an1_panel_review_v1` hashes actual weight/trace/output bytes (never trusting `.claimed_*_root` attributes),
//!      routes oversized (`> 512` B) or unserviceable records to `MaterialsUnauthenticated` / `MaterialsUnavailable`
//!      so the selected Panel files a qualified `material_notice` hold (`AN1-16`, `A57-11`), and otherwise replays
//!      the full execution to approve or emit a first-divergence Court refutation (`AN1-07`, `AN1-08`).
//!
//! 5. **Atomic Final Gate & Same-Batch Refutation / Hold Precedence (`RFC-0059 §10`, `AN1-08`, `AN1-09`, `AN1-11`, `AN1-12`)**:
//!    - `apply_batch` applies qualified `material_notice` holds and `ComputationRefuted` verdicts BEFORE
//!      evaluating `finalize`. Timeout never converts an unresolved material hold or unverified claim into success.
//!    - `finalize` re-verifies current `NativeContext` and atomically consumes `slot_id`, `attempt_id (s)`,
//!      `ComputationalWorkKey`, and (for `"AN1-TEST-USEFUL-ONCE-1"`) `BaseWorkKey`, with full `rollback_last_final` support.

use crate::palw_staged_work_commitments_v1::{SWC1_MAX_DIFFICULTY_D_V1, swc1_difficulty_threshold_be_v1};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Pinned reference profile constants (`AN1-INDEPENDENT-TOY-20261118`).
pub const AN1_PROFILE_NAME_V1: &str = "AN1-INDEPENDENT-TOY-20261118";
pub const AN1_PROFILE_STATUS_V1: &str = "DRAFT_NOT_ACTIVATED";
pub const AN1_VOCABULARY_V1: usize = 259;
pub const AN1_DIMENSION_V1: usize = 2;
pub const AN1_GENERATION_LENGTH_V1: usize = 2;
pub const AN1_WEIGHT_COUNT_V1: usize = 1060;
pub const AN1_TOKEN_BOS_V1: i64 = 0;
pub const AN1_TOKEN_POW_V1: i64 = 1;
pub const AN1_TOKEN_SEP_V1: i64 = 2;
pub const AN1_TOKEN_BYTE_OFFSET_V1: i64 = 3;
pub const AN1_MAX_JOB_TOKENS_V1: usize = 8;
pub const AN1_MAX_LEAF_BYTES_V1: usize = 512;

/// Canonical JSON string of `PROFILE` matching `an1_reference.py` byte-for-byte.
pub const AN1_CANONICAL_PROFILE_JSON_V1: &str = concat!(
    r#"{"attention":"last-position causal; score=max(0,dot(Q,K))+1","#,
    r#""dimension":2,"#,
    r#""encoding":"canonical JSON UTF-8, sorted keys, compact separators, integers only","#,
    r#""generation_length":2,"#,
    r#""hash":"SHA-256 with length-delimited fields and role-specific domains","#,
    r#""integer":"checked-signed-64; floor division; no floating point","#,
    r#""maximum_job_tokens":8,"#,
    r#""maximum_record_bytes":512,"#,
    r#""name":"AN1-INDEPENDENT-TOY-20261118","#,
    r#""selection":"smallest token index attaining maximum logit","#,
    r#""status":"DRAFT_NOT_ACTIVATED","#,
    r#""tokens":{"BOS":0,"POW":1,"SEP":2,"byte_offset":3},"#,
    r#""tree":"next-power-of-two, canonical padding, scoped every hash","#,
    r#""vocabulary":259,"#,
    r#""weight_count":1060,"#,
    r#""weight_layout":{"F1":[530,534],"F2":[534,538],"K":[522,526],"Q":[518,522],"V":[526,530],"bias1":[538,540],"bias2":[540,542],"embedding":[0,518],"output":[542,1060]}}"#,
);

/// Pinned `PROFILE_ID` hex (`H("AN1-profile", encode(PROFILE))`).
pub const AN1_PINNED_PROFILE_ID_HEX_V1: &str = "55f9da28ab73bf3a6ac6d52d7c13dace2a81614fc5a9e2b0a953418f060a0310";

/// Errors returned by `PALW-AN1` functions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum An1ErrorV1 {
    NonCanonicalOrOverflowingSigned64,
    NonCanonicalUnsigned64,
    NonCanonicalHex32,
    NonCanonicalRecord,
    IncorrectRecordIndex { expected: usize, got: i64 },
    EmptyTreeNotAllowed,
    OversizedOrNonCanonicalTreeRecord { index: usize, len: usize },
    InvalidPaddingPosition(usize),
    InvalidOpeningIndex(usize),
    UnsupportedInputLength(usize),
    NonCausalProgramArgument { instruction: usize, arg: usize },
    NonTokenEmbeddingInput(i64),
    InputTokenOutOfRange(i64),
    NonPositiveAttentionDenominator(i64),
    UnknownInstructionOp(String),
    InvalidWeightCount(usize),
    InvalidJobLength(usize),
    InvalidJobToken(i64),
    InvalidContextSchema(String),
    UnsupportedProfileOrAccounting,
    InvalidStrictTarget257,
    InvalidContextName(String),
    InvalidBeaconOutput,
    InvalidClaimSchema(String),
    JobRootMismatch,
    ModelRootMismatch,
    ClaimDoesNotMatchNativeRegisteredStatement,
    RegistrationMustPrecedeBeaconAccess,
    DuplicateRegistrationOrSlot,
    OriginalSealOwnerTimingOrTicketInvalid,
    ClaimMustBeSealedBeforeUniqueLaterPanelEvent,
    NotSelectedPanelActor,
    UnqualifiedMaterialNotice(String),
    MaterialScopeOutsideAuthenticatedClaim,
    InvalidDifficulty(u128),
}

/// Dynamic scalar value used to test and enforce strict canonical integer typing (`AN1-15`, `A56-09`, `A57-12`).
///
/// Rejects `Float` (`6153.0`, `0.0`), `Bool` (`true`, `false`), `Text`, and `Null` wherever a canonical
/// signed-64 or unsigned-64 integer is required.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum An1ScalarValueV1 {
    Int(i128),
    Float(f64),
    Bool(bool),
    Text(String),
    Null,
}

impl An1ScalarValueV1 {
    pub fn as_checked_i64(&self) -> Result<i64, An1ErrorV1> {
        match *self {
            Self::Int(v) if v >= i64::MIN as i128 && v <= i64::MAX as i128 => Ok(v as i64),
            _ => Err(An1ErrorV1::NonCanonicalOrOverflowingSigned64),
        }
    }

    pub fn as_checked_u64(&self) -> Result<u64, An1ErrorV1> {
        match *self {
            Self::Int(v) if v >= 0 && v <= u64::MAX as i128 => Ok(v as u64),
            _ => Err(An1ErrorV1::NonCanonicalUnsigned64),
        }
    }

    pub fn as_checked_usize(&self) -> Result<usize, An1ErrorV1> {
        let u = self.as_checked_u64()?;
        usize::try_from(u).map_err(|_| An1ErrorV1::NonCanonicalUnsigned64)
    }
}

/// Compute `H(domain, *parts)` with 8-byte big-endian length prefixes (`an1_reference.py`).
pub fn an1_h256_v1(domain: &str, parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    let dom_bytes = domain.as_bytes();
    hasher.update((dom_bytes.len() as u64).to_be_bytes());
    hasher.update(dom_bytes);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    hasher.finalize().into()
}

/// Convert `[u8; 32]` to lowercase 64-char hex string.
pub fn an1_bytes32_to_hex_v1(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(64);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// Parse and validate a canonical lowercase 64-character hex string into `[u8; 32]`.
pub fn an1_hex32_to_bytes_v1(s: &str) -> Result<[u8; 32], An1ErrorV1> {
    if s.len() != 64 {
        return Err(An1ErrorV1::NonCanonicalHex32);
    }
    let mut out = [0u8; 32];
    for (idx, chunk) in s.as_bytes().chunks_exact(2).enumerate() {
        let hi = match chunk[0] {
            b'0'..=b'9' => chunk[0] - b'0',
            b'a'..=b'f' => chunk[0] - b'a' + 10,
            _ => return Err(An1ErrorV1::NonCanonicalHex32),
        };
        let lo = match chunk[1] {
            b'0'..=b'9' => chunk[1] - b'0',
            b'a'..=b'f' => chunk[1] - b'a' + 10,
            _ => return Err(An1ErrorV1::NonCanonicalHex32),
        };
        out[idx] = (hi << 4) | lo;
    }
    Ok(out)
}

/// Compute `PROFILE_ID` (`55f9da28ab73bf3a6ac6d52d7c13dace2a81614fc5a9e2b0a953418f060a0310`).
pub fn an1_profile_id_bytes_v1() -> [u8; 32] {
    an1_h256_v1("AN1-profile", &[AN1_CANONICAL_PROFILE_JSON_V1.as_bytes()])
}

pub fn an1_profile_id_hex_v1() -> String {
    an1_bytes32_to_hex_v1(&an1_profile_id_bytes_v1())
}

/// Encode `{"i":index,"v":value}` in canonical compact sorted-key JSON.
#[inline]
pub fn an1_encode_iv_record_v1(index: usize, value: i64) -> Vec<u8> {
    format!(r#"{{"i":{index},"v":{value}}}"#).into_bytes()
}

/// Encode canonical padding leaf `{"padding":index,"value":0}`.
#[inline]
pub fn an1_pad_record_v1(index: usize) -> Vec<u8> {
    format!(r#"{{"padding":{index},"value":0}}"#).into_bytes()
}

/// Parse and strictly validate `{"i":expected_index,"v":signed64_int}` from raw leaf bytes (`an1_reference.py::parse_value`).
///
/// Rejects extra keys, whitespace, floats (`0.0`), booleans (`true`/`false`), wrong index, or `i64` overflow.
pub fn an1_parse_iv_record_v1(raw: &[u8], expected_index: usize) -> Result<i64, An1ErrorV1> {
    let s = std::str::from_utf8(raw).map_err(|_| An1ErrorV1::NonCanonicalRecord)?;
    let prefix = format!(r#"{{"i":{expected_index},"v":"#);
    if !s.starts_with(&prefix) || !s.ends_with('}') {
        return Err(An1ErrorV1::NonCanonicalRecord);
    }
    let val_str = &s[prefix.len()..s.len() - 1];
    if val_str.is_empty() || val_str.contains('.') || val_str == "true" || val_str == "false" {
        return Err(An1ErrorV1::NonCanonicalRecord);
    }
    let value: i64 = val_str.parse().map_err(|_| An1ErrorV1::NonCanonicalOrOverflowingSigned64)?;
    if an1_encode_iv_record_v1(expected_index, value) != raw {
        return Err(An1ErrorV1::NonCanonicalRecord);
    }
    Ok(value)
}

/// Strict 257-bit target threshold `theta` (`0 <= theta <= 2^256`, `RFC-0059 §5`, `RFC-0056 §17.2`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum An1TargetThreshold257V1 {
    /// Strict threshold in `0..=2^256 - 1` (represented as big-endian 256-bit integer).
    BelowTwoPow256([u8; 32]),
    /// Exact `2^256` (`D = 1`), under which every 256-bit ticket `0..2^256` passes `ticket < theta`.
    MaxTwoPow256,
}

impl An1TargetThreshold257V1 {
    /// Compute `theta(D) = floor((2^256 - 1) / D) + 1` for `1 <= D <= 2^120 - 1` (`RFC-0056 §17.2`).
    pub fn from_difficulty_d(difficulty_d: u128) -> Result<Self, An1ErrorV1> {
        if difficulty_d == 0 || difficulty_d > SWC1_MAX_DIFFICULTY_D_V1 {
            return Err(An1ErrorV1::InvalidDifficulty(difficulty_d));
        }
        if difficulty_d == 1 {
            return Ok(Self::MaxTwoPow256);
        }
        let mut t_be = swc1_difficulty_threshold_be_v1(difficulty_d).map_err(|_| An1ErrorV1::InvalidDifficulty(difficulty_d))?;
        // Add 1 to t_be in big-endian 256-bit arithmetic
        let mut carry = 1u16;
        for b in t_be.iter_mut().rev() {
            let sum = (*b as u16) + carry;
            *b = (sum & 0xFF) as u8;
            carry = sum >> 8;
            if carry == 0 {
                break;
            }
        }
        if carry != 0 { Ok(Self::MaxTwoPow256) } else { Ok(Self::BelowTwoPow256(t_be)) }
    }

    /// Evaluate `ticket < theta` in exact 257-bit arithmetic (`RFC-0059 §5`).
    #[inline]
    pub fn admits_ticket_be(&self, ticket_be: &[u8; 32]) -> bool {
        match self {
            Self::MaxTwoPow256 => true,
            Self::BelowTwoPow256(theta_be) => ticket_be < theta_be,
        }
    }

    /// Format as decimal string for canonical JSON encoding in `An1RegistrationContextV1`.
    pub fn to_decimal_string(&self) -> String {
        match self {
            Self::MaxTwoPow256 => "115792089237316195423570985008687907853269984665640564039457584007913129639936".to_owned(),
            Self::BelowTwoPow256(bytes) => {
                // Convert 32-byte big-endian unsigned integer to base-10 string
                let mut digits = vec![0u8];
                for &byte in bytes {
                    let mut carry = byte as u32;
                    for d in &mut digits {
                        let val = (*d as u32) * 256 + carry;
                        *d = (val % 10) as u8;
                        carry = val / 10;
                    }
                    while carry > 0 {
                        digits.push((carry % 10) as u8);
                        carry /= 10;
                    }
                }
                while digits.len() > 1 && *digits.last().unwrap() == 0 {
                    digits.pop();
                }
                digits.iter().rev().map(|d| (b'0' + *d) as char).collect()
            }
        }
    }
}

/// Five-phase `PALW-AN1` timing plan (`close_reg < beacon_confirm < seal_original < panel_select < hard_deadline`, `RFC-0056 §17.4`, `A56-09`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct An1TimingPlanV1 {
    pub close_reg: u64,
    pub beacon_confirm: u64,
    pub seal_original: u64,
    pub panel_select: u64,
    pub hard_deadline: u64,
}

impl An1TimingPlanV1 {
    pub const fn default_test_plan() -> Self {
        Self { close_reg: 10, beacon_confirm: 20, seal_original: 50, panel_select: 100, hard_deadline: 300 }
    }

    pub fn validate(&self) -> Result<(), An1ErrorV1> {
        if !(self.close_reg < self.beacon_confirm
            && self.beacon_confirm < self.seal_original
            && self.seal_original < self.panel_select
            && self.panel_select < self.hard_deadline)
        {
            return Err(An1ErrorV1::RegistrationMustPrecedeBeaconAccess);
        }
        Ok(())
    }
}

/// Merkle opening (`Opening`, `RFC-0059 §6.3`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct An1OpeningV1 {
    pub index: usize,
    pub payload: Vec<u8>,
    pub siblings: Vec<[u8; 32]>,
}

/// Context-bound padded Merkle tree (`Tree`, `RFC-0059 §6.3`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct An1TreeV1 {
    pub kind: String,
    pub scope: String,
    pub n: usize,
    pub l: usize,
    pub records: Vec<Vec<u8>>,
    pub levels: Vec<Vec<[u8; 32]>>,
    pub hash_inputs: Vec<Vec<u8>>,
    pub root: [u8; 32],
}

impl An1TreeV1 {
    pub fn build(
        kind: &str,
        scope: &str,
        records: &[Vec<u8>],
        padding_overrides: Option<&BTreeMap<usize, Vec<u8>>>,
    ) -> Result<Self, An1ErrorV1> {
        if records.is_empty() {
            return Err(An1ErrorV1::EmptyTreeNotAllowed);
        }
        for (i, rec) in records.iter().enumerate() {
            if rec.len() > AN1_MAX_LEAF_BYTES_V1 {
                return Err(An1ErrorV1::OversizedOrNonCanonicalTreeRecord { index: i, len: rec.len() });
            }
        }
        let n = records.len();
        let l = if n <= 1 { 1 } else { 1usize << (usize::BITS - (n - 1).leading_zeros()) };
        if let Some(overrides) = padding_overrides {
            for (&idx, payload) in overrides {
                if idx < n || idx >= l {
                    return Err(An1ErrorV1::InvalidPaddingPosition(idx));
                }
                if payload.len() > AN1_MAX_LEAF_BYTES_V1 {
                    return Err(An1ErrorV1::OversizedOrNonCanonicalTreeRecord { index: idx, len: payload.len() });
                }
            }
        }
        let mut all_records = Vec::with_capacity(l);
        all_records.extend_from_slice(records);
        for i in n..l {
            let pad = padding_overrides.and_then(|m| m.get(&i).cloned()).unwrap_or_else(|| an1_pad_record_v1(i));
            all_records.push(pad);
        }
        let mut hash_inputs = Vec::with_capacity(2 * l - 1);
        let mut leaf_hashes = Vec::with_capacity(l);
        for (i, payload) in all_records.iter().enumerate() {
            let pre = Self::leaf_preimage(kind, scope, n, l, i, payload);
            let h: [u8; 32] = Sha256::digest(&pre).into();
            hash_inputs.push(pre);
            leaf_hashes.push(h);
        }
        let mut levels = vec![leaf_hashes];
        let mut height = 1usize;
        while levels.last().unwrap().len() > 1 {
            let old = levels.last().unwrap();
            let mut next = Vec::with_capacity(old.len() / 2);
            for index in 0..(old.len() / 2) {
                let pre = Self::node_preimage(kind, scope, n, l, height, index, &old[2 * index], &old[2 * index + 1]);
                let h: [u8; 32] = Sha256::digest(&pre).into();
                hash_inputs.push(pre);
                next.push(h);
            }
            levels.push(next);
            height += 1;
        }
        let root = levels.last().unwrap()[0];
        Ok(Self { kind: kind.to_owned(), scope: scope.to_owned(), n, l, records: all_records, levels, hash_inputs, root })
    }

    pub fn preimage(domain: &str, kind: &str, scope: &str, n: usize, l: usize, coordinates: &[usize], extra: &[&[u8]]) -> Vec<u8> {
        let mut coords_json = format!("[{n},{l}");
        for c in coordinates {
            coords_json.push(',');
            coords_json.push_str(&c.to_string());
        }
        coords_json.push(']');
        let fixed_parts: [&[u8]; 4] = [domain.as_bytes(), kind.as_bytes(), scope.as_bytes(), coords_json.as_bytes()];
        let mut out = Vec::with_capacity(128);
        for p in fixed_parts.into_iter().chain(extra.iter().copied()) {
            out.extend_from_slice(&(p.len() as u64).to_be_bytes());
            out.extend_from_slice(p);
        }
        out
    }

    pub fn leaf_preimage(kind: &str, scope: &str, n: usize, l: usize, index: usize, payload: &[u8]) -> Vec<u8> {
        let domain = if index < n { "AN1-real-leaf" } else { "AN1-padding-leaf" };
        Self::preimage(domain, kind, scope, n, l, &[index], &[payload])
    }

    pub fn node_preimage(
        kind: &str,
        scope: &str,
        n: usize,
        l: usize,
        height: usize,
        index: usize,
        left: &[u8; 32],
        right: &[u8; 32],
    ) -> Vec<u8> {
        Self::preimage("AN1-internal", kind, scope, n, l, &[height, index], &[left, right])
    }

    pub fn root_hex(&self) -> String {
        an1_bytes32_to_hex_v1(&self.root)
    }

    pub fn opening(&self, index: usize) -> Result<An1OpeningV1, An1ErrorV1> {
        if index >= self.l {
            return Err(An1ErrorV1::InvalidOpeningIndex(index));
        }
        let mut j = index;
        let mut siblings = Vec::with_capacity(self.levels.len().saturating_sub(1));
        for level in &self.levels[..self.levels.len() - 1] {
            siblings.push(level[j ^ 1]);
            j /= 2;
        }
        Ok(An1OpeningV1 { index, payload: self.records[index].clone(), siblings })
    }
}

/// Verify a Merkle opening against `root` (`an1_reference.py::verify_opening`).
pub fn an1_verify_opening_v1(kind: &str, scope: &str, n: usize, root: &[u8; 32], opening: &An1OpeningV1) -> bool {
    if n < 1 {
        return false;
    }
    let l = if n <= 1 { 1 } else { 1usize << (usize::BITS - (n - 1).leading_zeros()) };
    let expected_depth = l.trailing_zeros() as usize;
    if opening.index >= l || opening.siblings.len() != expected_depth || opening.payload.len() > AN1_MAX_LEAF_BYTES_V1 {
        return false;
    }
    let mut idx = opening.index;
    let pre = An1TreeV1::leaf_preimage(kind, scope, n, l, idx, &opening.payload);
    let mut digest: [u8; 32] = Sha256::digest(&pre).into();
    for (h_zero, sibling) in opening.siblings.iter().enumerate() {
        let height = h_zero + 1;
        let (left, right) = if idx % 2 == 0 { (&digest, sibling) } else { (sibling, &digest) };
        idx /= 2;
        let node_pre = An1TreeV1::node_preimage(kind, scope, n, l, height, idx, left, right);
        digest = Sha256::digest(&node_pre).into();
    }
    &digest == root
}

/// Opcode in the `PALW-AN1` causal instruction graph (`RFC-0059 §6.1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum An1InstructionOpV1 {
    Const,
    Input,
    Weight,
    Embed,
    Add,
    Mul,
    Max0,
    Max,
    Div,
    ArgmaxUpdate,
}

impl An1InstructionOpV1 {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Const => "CONST",
            Self::Input => "INPUT",
            Self::Weight => "WEIGHT",
            Self::Embed => "EMBED",
            Self::Add => "ADD",
            Self::Mul => "MUL",
            Self::Max0 => "MAX0",
            Self::Max => "MAX",
            Self::Div => "DIV",
            Self::ArgmaxUpdate => "ARGMAX_UPDATE",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, An1ErrorV1> {
        match s {
            "CONST" => Ok(Self::Const),
            "INPUT" => Ok(Self::Input),
            "WEIGHT" => Ok(Self::Weight),
            "EMBED" => Ok(Self::Embed),
            "ADD" => Ok(Self::Add),
            "MUL" => Ok(Self::Mul),
            "MAX0" => Ok(Self::Max0),
            "MAX" => Ok(Self::Max),
            "DIV" => Ok(Self::Div),
            "ARGMAX_UPDATE" => Ok(Self::ArgmaxUpdate),
            other => Err(An1ErrorV1::UnknownInstructionOp(other.to_owned())),
        }
    }

    pub const fn arity(self) -> (usize, usize) {
        match self {
            Self::Const | Self::Input | Self::Weight => (0, 1),
            Self::Embed => (1, 1),
            Self::Add | Self::Mul | Self::Max | Self::Div => (2, 0),
            Self::Max0 => (1, 0),
            Self::ArgmaxUpdate => (3, 1),
        }
    }
}

/// Single instruction in the compiled `PALW-AN1` program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct An1InstructionV1 {
    pub i: usize,
    pub op: An1InstructionOpV1,
    pub args: Vec<usize>,
    pub imm: Vec<i64>,
}

impl An1InstructionV1 {
    /// Canonical JSON encoding matching `encode({"i": i, "op": op, "args": args, "imm": imm})` in `an1_reference.py`.
    pub fn encode_canonical_json(&self) -> Vec<u8> {
        let args_str: Vec<String> = self.args.iter().map(usize::to_string).collect();
        let imm_str: Vec<String> = self.imm.iter().map(i64::to_string).collect();
        format!(r#"{{"args":[{}],"i":{},"imm":[{}],"op":"{}"}}"#, args_str.join(","), self.i, imm_str.join(","), self.op.as_str())
            .into_bytes()
    }

    /// Parse and validate an authenticated instruction record at `expected_index` (`an1_reference.py::parse_instruction`).
    pub fn parse_and_validate(raw: &[u8], expected_index: usize, input_length: usize) -> Result<Self, An1ErrorV1> {
        let s = std::str::from_utf8(raw).map_err(|_| An1ErrorV1::NonCanonicalRecord)?;
        // Format is `{"args":[...],"i":...,"imm":[...],"op":"..."}`
        let rest = s.strip_prefix(r#"{"args":["#).ok_or(An1ErrorV1::NonCanonicalRecord)?;
        let (args_part, rest) = rest.split_once(r#"],"i":"#).ok_or(An1ErrorV1::NonCanonicalRecord)?;
        let (i_part, rest) = rest.split_once(r#","imm":["#).ok_or(An1ErrorV1::NonCanonicalRecord)?;
        let (imm_part, rest) = rest.split_once(r#"],"op":""#).ok_or(An1ErrorV1::NonCanonicalRecord)?;
        let op_part = rest.strip_suffix(r#""}"#).ok_or(An1ErrorV1::NonCanonicalRecord)?;

        let parsed_i: usize = i_part.parse().map_err(|_| An1ErrorV1::NonCanonicalRecord)?;
        if parsed_i != expected_index {
            return Err(An1ErrorV1::IncorrectRecordIndex { expected: expected_index, got: parsed_i as i64 });
        }
        let op = An1InstructionOpV1::from_str(op_part)?;
        let args: Vec<usize> = if args_part.is_empty() {
            Vec::new()
        } else {
            args_part
                .split(',')
                .map(|tok| tok.parse::<usize>().map_err(|_| An1ErrorV1::NonCanonicalRecord))
                .collect::<Result<Vec<_>, _>>()?
        };
        let imm: Vec<i64> = if imm_part.is_empty() {
            Vec::new()
        } else {
            imm_part
                .split(',')
                .map(|tok| tok.parse::<i64>().map_err(|_| An1ErrorV1::NonCanonicalRecord))
                .collect::<Result<Vec<_>, _>>()?
        };
        if (args.len(), imm.len()) != op.arity() {
            return Err(An1ErrorV1::NonCanonicalRecord);
        }
        for &arg in &args {
            if arg >= expected_index {
                return Err(An1ErrorV1::NonCausalProgramArgument { instruction: expected_index, arg });
            }
        }
        match op {
            An1InstructionOpV1::Const => {}
            An1InstructionOpV1::Input => {
                if imm[0] < 0 || (imm[0] as usize) >= input_length {
                    return Err(An1ErrorV1::NonCanonicalRecord);
                }
            }
            An1InstructionOpV1::Weight => {
                if imm[0] < 0 || (imm[0] as usize) >= AN1_WEIGHT_COUNT_V1 {
                    return Err(An1ErrorV1::NonCanonicalRecord);
                }
            }
            An1InstructionOpV1::Embed => {
                if imm[0] < 0 || imm[0] >= 2 {
                    return Err(An1ErrorV1::NonCanonicalRecord);
                }
            }
            An1InstructionOpV1::ArgmaxUpdate => {
                if imm[0] < 1 || (imm[0] as usize) >= AN1_VOCABULARY_V1 {
                    return Err(An1ErrorV1::NonCanonicalRecord);
                }
            }
            _ => {}
        }
        let ins = Self { i: expected_index, op, args, imm };
        if ins.encode_canonical_json() != raw {
            return Err(An1ErrorV1::NonCanonicalRecord);
        }
        Ok(ins)
    }
}

/// Compiled `PALW-AN1` program and its authenticated Merkle tree (`Program`, `RFC-0059 §6.1`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct An1ProgramV1 {
    pub instructions: Vec<An1InstructionV1>,
    pub output_refs: Vec<usize>,
    pub input_length: usize,
    pub tree: An1TreeV1,
}

/// Compile the causal instruction graph for `input_length` (`35..=43`, `an1_reference.py::compile_program`).
pub fn an1_compile_program_v1(input_length: usize) -> Result<An1ProgramV1, An1ErrorV1> {
    if !(35..=43).contains(&input_length) {
        return Err(An1ErrorV1::UnsupportedInputLength(input_length));
    }
    let mut instructions: Vec<An1InstructionV1> = Vec::with_capacity(6200);
    let mut put = |op: An1InstructionOpV1, args: Vec<usize>, imm: Vec<i64>| -> Result<usize, An1ErrorV1> {
        let i = instructions.len();
        for &j in &args {
            if j >= i {
                return Err(An1ErrorV1::NonCausalProgramArgument { instruction: i, arg: j });
            }
        }
        instructions.push(An1InstructionV1 { i, op, args, imm });
        Ok(i)
    };

    let zero = put(An1InstructionOpV1::Const, vec![], vec![0])?;
    let one = put(An1InstructionOpV1::Const, vec![], vec![1])?;
    let mut tokens = Vec::with_capacity(input_length + AN1_GENERATION_LENGTH_V1);
    for i in 0..input_length {
        tokens.push(put(An1InstructionOpV1::Input, vec![], vec![i as i64])?);
    }
    let mut output_refs = Vec::with_capacity(AN1_GENERATION_LENGTH_V1);

    for _ in 0..AN1_GENERATION_LENGTH_V1 {
        let mut embeddings = Vec::with_capacity(tokens.len());
        for &tok in &tokens {
            let e0 = put(An1InstructionOpV1::Embed, vec![tok], vec![0])?;
            let e1 = put(An1InstructionOpV1::Embed, vec![tok], vec![1])?;
            embeddings.push([e0, e1]);
        }

        let mut keys = Vec::with_capacity(embeddings.len());
        for e in &embeddings {
            let mut out_row = [0usize; 2];
            for row in 0..2 {
                let mut prods = Vec::with_capacity(2);
                for col in 0..2 {
                    let w = put(An1InstructionOpV1::Weight, vec![], vec![(522 + row * 2 + col) as i64])?;
                    prods.push(put(An1InstructionOpV1::Mul, vec![e[col], w], vec![])?);
                }
                out_row[row] = put(An1InstructionOpV1::Add, prods, vec![])?;
            }
            keys.push(out_row);
        }

        let mut values = Vec::with_capacity(embeddings.len());
        for e in &embeddings {
            let mut out_row = [0usize; 2];
            for row in 0..2 {
                let mut prods = Vec::with_capacity(2);
                for col in 0..2 {
                    let w = put(An1InstructionOpV1::Weight, vec![], vec![(526 + row * 2 + col) as i64])?;
                    prods.push(put(An1InstructionOpV1::Mul, vec![e[col], w], vec![])?);
                }
                out_row[row] = put(An1InstructionOpV1::Add, prods, vec![])?;
            }
            values.push(out_row);
        }

        let last_emb = *embeddings.last().unwrap();
        let mut query = [0usize; 2];
        for row in 0..2 {
            let mut prods = Vec::with_capacity(2);
            for col in 0..2 {
                let w = put(An1InstructionOpV1::Weight, vec![], vec![(518 + row * 2 + col) as i64])?;
                prods.push(put(An1InstructionOpV1::Mul, vec![last_emb[col], w], vec![])?);
            }
            query[row] = put(An1InstructionOpV1::Add, prods, vec![])?;
        }

        let mut scores = Vec::with_capacity(keys.len());
        for key in &keys {
            let m0 = put(An1InstructionOpV1::Mul, vec![query[0], key[0]], vec![])?;
            let m1 = put(An1InstructionOpV1::Mul, vec![query[1], key[1]], vec![])?;
            let dot = put(An1InstructionOpV1::Add, vec![m0, m1], vec![])?;
            let max0 = put(An1InstructionOpV1::Max0, vec![dot], vec![])?;
            scores.push(put(An1InstructionOpV1::Add, vec![max0, one], vec![])?);
        }

        let mut denominator = zero;
        let mut numerators = [zero, zero];
        for (j, &score) in scores.iter().enumerate() {
            denominator = put(An1InstructionOpV1::Add, vec![denominator, score], vec![])?;
            for k in 0..2 {
                let prod = put(An1InstructionOpV1::Mul, vec![score, values[j][k]], vec![])?;
                numerators[k] = put(An1InstructionOpV1::Add, vec![numerators[k], prod], vec![])?;
            }
        }
        let attention = [
            put(An1InstructionOpV1::Div, vec![numerators[0], denominator], vec![])?,
            put(An1InstructionOpV1::Div, vec![numerators[1], denominator], vec![])?,
        ];
        let residual = [
            put(An1InstructionOpV1::Add, vec![attention[0], last_emb[0]], vec![])?,
            put(An1InstructionOpV1::Add, vec![attention[1], last_emb[1]], vec![])?,
        ];

        // matvec(residual, 530, bias=538) -> MAX0
        let mut pre_middle = [0usize; 2];
        for row in 0..2 {
            let mut prods = Vec::with_capacity(2);
            for col in 0..2 {
                let w = put(An1InstructionOpV1::Weight, vec![], vec![(530 + row * 2 + col) as i64])?;
                prods.push(put(An1InstructionOpV1::Mul, vec![residual[col], w], vec![])?);
            }
            let total = put(An1InstructionOpV1::Add, prods, vec![])?;
            let b_w = put(An1InstructionOpV1::Weight, vec![], vec![(538 + row) as i64])?;
            pre_middle[row] = put(An1InstructionOpV1::Add, vec![total, b_w], vec![])?;
        }
        let middle =
            [put(An1InstructionOpV1::Max0, vec![pre_middle[0]], vec![])?, put(An1InstructionOpV1::Max0, vec![pre_middle[1]], vec![])?];

        // matvec(middle, 534, bias=540)
        let mut projected = [0usize; 2];
        for row in 0..2 {
            let mut prods = Vec::with_capacity(2);
            for col in 0..2 {
                let w = put(An1InstructionOpV1::Weight, vec![], vec![(534 + row * 2 + col) as i64])?;
                prods.push(put(An1InstructionOpV1::Mul, vec![middle[col], w], vec![])?);
            }
            let total = put(An1InstructionOpV1::Add, prods, vec![])?;
            let b_w = put(An1InstructionOpV1::Weight, vec![], vec![(540 + row) as i64])?;
            projected[row] = put(An1InstructionOpV1::Add, vec![total, b_w], vec![])?;
        }

        let hidden = [
            put(An1InstructionOpV1::Add, vec![projected[0], residual[0]], vec![])?,
            put(An1InstructionOpV1::Add, vec![projected[1], residual[1]], vec![])?,
        ];

        // logits = matvec(hidden, 542, rows=259)
        let mut logits = Vec::with_capacity(AN1_VOCABULARY_V1);
        for row in 0..AN1_VOCABULARY_V1 {
            let mut prods = Vec::with_capacity(2);
            for col in 0..2 {
                let w = put(An1InstructionOpV1::Weight, vec![], vec![(542 + row * 2 + col) as i64])?;
                prods.push(put(An1InstructionOpV1::Mul, vec![hidden[col], w], vec![])?);
            }
            logits.push(put(An1InstructionOpV1::Add, prods, vec![])?);
        }

        let mut best_index = zero;
        let mut best_value = logits[0];
        for i in 1..AN1_VOCABULARY_V1 {
            best_index = put(An1InstructionOpV1::ArgmaxUpdate, vec![best_index, best_value, logits[i]], vec![i as i64])?;
            best_value = put(An1InstructionOpV1::Max, vec![best_value, logits[i]], vec![])?;
        }
        output_refs.push(best_index);
        tokens.push(best_index);
    }

    let profile_id = an1_profile_id_bytes_v1();
    let len_bytes = input_length.to_string().into_bytes();
    let scope_bytes = an1_h256_v1("AN1-program-scope", &[&profile_id, &len_bytes]);
    let scope_hex = an1_bytes32_to_hex_v1(&scope_bytes);
    let encoded_records: Vec<Vec<u8>> = instructions.iter().map(An1InstructionV1::encode_canonical_json).collect();
    let tree = An1TreeV1::build("program", &scope_hex, &encoded_records, None)?;
    Ok(An1ProgramV1 { instructions, output_refs, input_length, tree })
}

/// Derive the expected model weight index for `instruction` from authenticated `parent_values` (`RFC-0059 §8.1`, `RFC-0057 §17.3`).
pub fn an1_expected_weight_index_v1(instruction: &An1InstructionV1, parent_values: &[i64]) -> Result<Option<usize>, An1ErrorV1> {
    match instruction.op {
        An1InstructionOpV1::Weight => Ok(Some(instruction.imm[0] as usize)),
        An1InstructionOpV1::Embed => {
            let token = *parent_values.first().ok_or(An1ErrorV1::NonTokenEmbeddingInput(-1))?;
            if token < 0 || (token as usize) >= AN1_VOCABULARY_V1 {
                return Err(An1ErrorV1::NonTokenEmbeddingInput(token));
            }
            Ok(Some((token as usize) * 2 + (instruction.imm[0] as usize)))
        }
        _ => Ok(None),
    }
}

/// Evaluate a single instruction in checked signed-64 arithmetic (`an1_reference.py::evaluate`).
pub fn an1_evaluate_instruction_v1(
    instruction: &An1InstructionV1,
    parents: &[i64],
    input_token: Option<i64>,
    weight: Option<i64>,
) -> Result<i64, An1ErrorV1> {
    match instruction.op {
        An1InstructionOpV1::Const => Ok(instruction.imm[0]),
        An1InstructionOpV1::Input => {
            let tok = input_token.ok_or(An1ErrorV1::InputTokenOutOfRange(-1))?;
            if tok < 0 || (tok as usize) >= AN1_VOCABULARY_V1 {
                return Err(An1ErrorV1::InputTokenOutOfRange(tok));
            }
            Ok(tok)
        }
        An1InstructionOpV1::Weight | An1InstructionOpV1::Embed => weight.ok_or(An1ErrorV1::NonCanonicalOrOverflowingSigned64),
        An1InstructionOpV1::Add => parents[0].checked_add(parents[1]).ok_or(An1ErrorV1::NonCanonicalOrOverflowingSigned64),
        An1InstructionOpV1::Mul => parents[0].checked_mul(parents[1]).ok_or(An1ErrorV1::NonCanonicalOrOverflowingSigned64),
        An1InstructionOpV1::Max0 => Ok(parents[0].max(0)),
        An1InstructionOpV1::Max => Ok(parents[0].max(parents[1])),
        An1InstructionOpV1::Div => {
            if parents[1] <= 0 {
                return Err(An1ErrorV1::NonPositiveAttentionDenominator(parents[1]));
            }
            Ok(parents[0].div_euclid(parents[1]))
        }
        An1InstructionOpV1::ArgmaxUpdate => {
            if parents[2] > parents[1] {
                Ok(instruction.imm[0])
            } else {
                Ok(parents[0])
            }
        }
    }
}

/// Build the canonical model weight Merkle tree (`an1_reference.py::model_tree`).
pub fn an1_model_tree_v1(weights: &[i64]) -> Result<An1TreeV1, An1ErrorV1> {
    if weights.len() != AN1_WEIGHT_COUNT_V1 {
        return Err(An1ErrorV1::InvalidWeightCount(weights.len()));
    }
    let records: Vec<Vec<u8>> = weights.iter().enumerate().map(|(i, &v)| an1_encode_iv_record_v1(i, v)).collect();
    An1TreeV1::build("model", &an1_profile_id_hex_v1(), &records, None)
}

/// Generate the pinned deterministic test weights (`seed = 23` by default, `an1_reference.py::deterministic_weights`).
pub fn an1_deterministic_weights_v1(seed: i64) -> Vec<i64> {
    let mut out = Vec::with_capacity(AN1_WEIGHT_COUNT_V1);
    for i in 0..AN1_WEIGHT_COUNT_V1 {
        let pair_json = format!("[{seed},{i}]").into_bytes();
        let digest = an1_h256_v1("AN1-test-weight", &[&pair_json]);
        // Compute uint256_be(digest) % 5 - 2
        let mut rem = 0u64;
        for &b in &digest {
            rem = ((rem << 8) | (b as u64)) % 5;
        }
        out.push((rem as i64) - 2);
    }
    out
}

/// Build the canonical job token Merkle tree (`an1_reference.py::job_tree`).
pub fn an1_job_tree_v1(job_tokens: &[i64]) -> Result<An1TreeV1, An1ErrorV1> {
    if job_tokens.is_empty() || job_tokens.len() > AN1_MAX_JOB_TOKENS_V1 {
        return Err(An1ErrorV1::InvalidJobLength(job_tokens.len()));
    }
    for &t in job_tokens {
        if t < 0 || (t as usize) >= AN1_VOCABULARY_V1 {
            return Err(An1ErrorV1::InvalidJobToken(t));
        }
    }
    let records: Vec<Vec<u8>> = job_tokens.iter().enumerate().map(|(i, &t)| an1_encode_iv_record_v1(i, t)).collect();
    An1TreeV1::build("job", &an1_profile_id_hex_v1(), &records, None)
}

/// Registration context `C` (`RFC-0059 §2.2`, `an1_reference.py::CONTEXT_FIELDS`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct An1RegistrationContextV1 {
    pub accounting_version: String,
    pub beacon_key: String,
    pub beacon_round: u64,
    pub chain: String,
    pub deadline: u64,
    pub epoch: u64,
    pub job_root: String,
    pub model_root: String,
    pub nonce: String,
    pub owner: String,
    pub panel_event: u64,
    pub profile_id: String,
    pub target: An1TargetThreshold257V1,
}

impl An1RegistrationContextV1 {
    pub fn validate(&self) -> Result<(), An1ErrorV1> {
        if self.profile_id != an1_profile_id_hex_v1() {
            return Err(An1ErrorV1::UnsupportedProfileOrAccounting);
        }
        if self.accounting_version != "AN1-TEST-UNIT-1" && self.accounting_version != "AN1-TEST-USEFUL-ONCE-1" {
            return Err(An1ErrorV1::UnsupportedProfileOrAccounting);
        }
        for h in [&self.owner, &self.model_root, &self.job_root, &self.nonce] {
            let _ = an1_hex32_to_bytes_v1(h)?;
        }
        for name in [&self.chain, &self.beacon_key] {
            if name.is_empty() || name.len() > 64 || !name.is_ascii() {
                return Err(An1ErrorV1::InvalidContextName("chain or beacon_key invalid".to_owned()));
            }
        }
        Ok(())
    }

    /// Encode `C` in canonical sorted-key compact JSON matching `an1_reference.py::encode(context)` byte-for-byte.
    pub fn encode_canonical_json(&self) -> Result<Vec<u8>, An1ErrorV1> {
        self.validate()?;
        Ok(format!(
            concat!(
                r#"{{"accounting_version":"{}","beacon_key":"{}","beacon_round":{},"chain":"{}","deadline":{},"epoch":{},"#,
                r#""job_root":"{}","model_root":"{}","nonce":"{}","owner":"{}","panel_event":{},"profile_id":"{}","target":{}}}"#
            ),
            self.accounting_version,
            self.beacon_key,
            self.beacon_round,
            self.chain,
            self.deadline,
            self.epoch,
            self.job_root,
            self.model_root,
            self.nonce,
            self.owner,
            self.panel_event,
            self.profile_id,
            self.target.to_decimal_string(),
        )
        .into_bytes())
    }

    /// Compute `RegKey = H("AN1-registration-key", encode(C))` (`RFC-0059 §2.3`).
    pub fn registration_key(&self) -> Result<[u8; 32], An1ErrorV1> {
        let enc = self.encode_canonical_json()?;
        Ok(an1_h256_v1("AN1-registration-key", &[&enc]))
    }

    /// Compute `slot_id = H("AN1-slot", encode(C))` (`RFC-0059 §2.3`).
    pub fn slot_id(&self) -> Result<[u8; 32], An1ErrorV1> {
        let enc = self.encode_canonical_json()?;
        Ok(an1_h256_v1("AN1-slot", &[&enc]))
    }

    pub fn slot_id_hex(&self) -> Result<String, An1ErrorV1> {
        Ok(an1_bytes32_to_hex_v1(&self.slot_id()?))
    }

    /// Compute `attempt_id (s) = H("AN1-attempt", encode(C), sigma)` (`RFC-0059 §2.3`).
    pub fn attempt_id(&self, sigma: &[u8; 32]) -> Result<[u8; 32], An1ErrorV1> {
        let enc = self.encode_canonical_json()?;
        Ok(an1_h256_v1("AN1-attempt", &[&enc, sigma]))
    }

    pub fn attempt_id_hex(&self, sigma: &[u8; 32]) -> Result<String, An1ErrorV1> {
        Ok(an1_bytes32_to_hex_v1(&self.attempt_id(sigma)?))
    }
}

/// Derive the challenge-prefixed actual input token sequence (`RFC-0059 §3.1`, `an1_reference.py::derive_input`).
///
/// `x = [BOS, POW] || [ByteToken[s_0], ..., ByteToken[s_31]] || [SEP] || job_tokens`.
pub fn an1_derive_input_tokens_v1(attempt_s: &[u8; 32], job_tokens: &[i64]) -> Result<Vec<i64>, An1ErrorV1> {
    let _ = an1_job_tree_v1(job_tokens)?;
    let mut out = Vec::with_capacity(35 + job_tokens.len());
    out.push(AN1_TOKEN_BOS_V1);
    out.push(AN1_TOKEN_POW_V1);
    for &b in attempt_s {
        out.push(AN1_TOKEN_BYTE_OFFSET_V1 + (b as i64));
    }
    out.push(AN1_TOKEN_SEP_V1);
    out.extend_from_slice(job_tokens);
    Ok(out)
}

/// Compute `ComputationalWorkKey` over actual challenge-prefixed input tokens (`RFC-0059 §2.3`, `an1_reference.py::computational_work_key`).
pub fn an1_computational_work_key_v1(
    context: &An1RegistrationContextV1,
    sigma: &[u8; 32],
    job_tokens: &[i64],
) -> Result<[u8; 32], An1ErrorV1> {
    let s = context.attempt_id(sigma)?;
    let tokens = an1_derive_input_tokens_v1(&s, job_tokens)?;
    let model_root = an1_hex32_to_bytes_v1(&context.model_root)?;
    let profile_id = an1_profile_id_bytes_v1();
    let tokens_str: Vec<String> = tokens.iter().map(i64::to_string).collect();
    let tokens_json = format!("[{}]", tokens_str.join(",")).into_bytes();
    Ok(an1_h256_v1("AN1-computational-work", &[&model_root, &profile_id, &tokens_json, b"2"]))
}

/// Compute `BaseWorkKey` over the underlying original job demand (`RFC-0059 §2.3`, `an1_reference.py::base_work_key`).
pub fn an1_base_work_key_v1(context: &An1RegistrationContextV1) -> Result<[u8; 32], An1ErrorV1> {
    context.validate()?;
    let model_root = an1_hex32_to_bytes_v1(&context.model_root)?;
    let job_root = an1_hex32_to_bytes_v1(&context.job_root)?;
    let profile_id = an1_profile_id_bytes_v1();
    Ok(an1_h256_v1("AN1-base-useful-work", &[&model_root, &job_root, &profile_id, b"2"]))
}

/// Compute `AN1` Ticket bytes (`H("AN1-ticket", s, R_x, R_y, R_tau)`, big-endian `uint256_be`, `RFC-0059 §5`).
pub fn an1_ticket_bytes_v1(attempt_s: &[u8; 32], input_root: &[u8; 32], output_root: &[u8; 32], trace_root: &[u8; 32]) -> [u8; 32] {
    an1_h256_v1("AN1-ticket", &[attempt_s, input_root, output_root, trace_root])
}

/// Original `Claim` statement sealed before Panel selection (`RFC-0059 §7`, `an1_reference.py::Claim`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct An1ClaimV1 {
    pub slot: String,
    pub owner: String,
    pub attempt: String,
    pub input_root: String,
    pub output_root: String,
    pub trace_root: String,
    pub program_root: String,
    pub trace_count: u64,
    pub output_count: u64,
}

impl An1ClaimV1 {
    pub fn validate(&self) -> Result<(), An1ErrorV1> {
        for h in [&self.slot, &self.owner, &self.attempt, &self.input_root, &self.output_root, &self.trace_root, &self.program_root] {
            let _ = an1_hex32_to_bytes_v1(h)?;
        }
        if self.trace_count == 0 || self.output_count == 0 {
            return Err(An1ErrorV1::InvalidClaimSchema("empty claim array".to_owned()));
        }
        Ok(())
    }

    /// Construct from dynamic scalar counts so non-canonical types (`float`, `bool`, negative, etc.) are rejected at admission (`AN1-15`).
    #[allow(clippy::too_many_arguments)]
    pub fn from_checked_scalars(
        slot: String,
        owner: String,
        attempt: String,
        input_root: String,
        output_root: String,
        trace_root: String,
        program_root: String,
        trace_count: &An1ScalarValueV1,
        output_count: &An1ScalarValueV1,
    ) -> Result<Self, An1ErrorV1> {
        let tc = trace_count.as_checked_u64()?;
        let oc = output_count.as_checked_u64()?;
        let claim =
            Self { slot, owner, attempt, input_root, output_root, trace_root, program_root, trace_count: tc, output_count: oc };
        claim.validate()?;
        Ok(claim)
    }

    pub fn encode_canonical_json(&self) -> Result<Vec<u8>, An1ErrorV1> {
        self.validate()?;
        Ok(format!(
            concat!(
                r#"{{"attempt":"{}","input_root":"{}","output_count":{},"output_root":"{}","owner":"{}","#,
                r#""program_root":"{}","slot":"{}","trace_count":{},"trace_root":"{}"}}"#
            ),
            self.attempt,
            self.input_root,
            self.output_count,
            self.output_root,
            self.owner,
            self.program_root,
            self.slot,
            self.trace_count,
            self.trace_root,
        )
        .into_bytes())
    }

    /// Compute `OriginalId = H("AN1-original", encode(claim))` (`RFC-0059 §2.3`).
    pub fn original_id_bytes(&self) -> Result<[u8; 32], An1ErrorV1> {
        let enc = self.encode_canonical_json()?;
        Ok(an1_h256_v1("AN1-original", &[&enc]))
    }

    pub fn original_id_hex(&self) -> Result<String, An1ErrorV1> {
        Ok(an1_bytes32_to_hex_v1(&self.original_id_bytes()?))
    }

    /// Compute 32-byte big-endian `ticket` (`H("AN1-ticket", s, R_x, R_y, R_tau)`).
    pub fn ticket_bytes(&self) -> Result<[u8; 32], An1ErrorV1> {
        self.validate()?;
        let s = an1_hex32_to_bytes_v1(&self.attempt)?;
        let rx = an1_hex32_to_bytes_v1(&self.input_root)?;
        let ry = an1_hex32_to_bytes_v1(&self.output_root)?;
        let rt = an1_hex32_to_bytes_v1(&self.trace_root)?;
        Ok(an1_ticket_bytes_v1(&s, &rx, &ry, &rt))
    }

    pub fn ticket_hex(&self) -> Result<String, An1ErrorV1> {
        Ok(an1_bytes32_to_hex_v1(&self.ticket_bytes()?))
    }
}

/// Execute `program` on `tokens` and `weights`, returning `(trace_values, output_tokens)` (`an1_reference.py::execute`).
pub fn an1_execute_program_v1(program: &An1ProgramV1, tokens: &[i64], weights: &[i64]) -> Result<(Vec<i64>, Vec<i64>), An1ErrorV1> {
    let _ = an1_model_tree_v1(weights)?;
    let mut values = Vec::with_capacity(program.instructions.len());
    for ins in &program.instructions {
        let parents: Vec<i64> = ins.args.iter().map(|&idx| values[idx]).collect();
        let weight_idx = an1_expected_weight_index_v1(ins, &parents)?;
        let weight = weight_idx.map(|wi| weights[wi]);
        let input_tok = if ins.op == An1InstructionOpV1::Input { Some(tokens[ins.imm[0] as usize]) } else { None };
        values.push(an1_evaluate_instruction_v1(ins, &parents, input_tok, weight)?);
    }
    let outputs: Vec<i64> = program.output_refs.iter().map(|&idx| values[idx]).collect();
    Ok((values, outputs))
}

/// Execution material bundle (`Material`, `an1_reference.py::Material`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct An1MaterialV1 {
    pub weights: Vec<i64>,
    pub trace_records: Vec<Vec<u8>>,
    pub output_records: Vec<Vec<u8>>,
    pub trace_padding: BTreeMap<usize, Vec<u8>>,
    /// Deliberately untrusted convenience fields — verification NEVER relies on them (`AN1-05`, `A57-04`).
    pub claimed_model_root: Option<String>,
    pub claimed_trace_root: Option<String>,
}

/// Build a canonical `(An1ClaimV1, An1MaterialV1)` from `(context, sigma, job_tokens, weights)` (`an1_reference.py::build_claim`).
pub fn an1_build_claim_v1(
    context: &An1RegistrationContextV1,
    sigma: &[u8; 32],
    job_tokens: &[i64],
    weights: &[i64],
) -> Result<(An1ClaimV1, An1MaterialV1), An1ErrorV1> {
    if an1_model_tree_v1(weights)?.root_hex() != context.model_root {
        return Err(An1ErrorV1::ModelRootMismatch);
    }
    if an1_job_tree_v1(job_tokens)?.root_hex() != context.job_root {
        return Err(An1ErrorV1::JobRootMismatch);
    }
    let s = context.attempt_id(sigma)?;
    let s_hex = an1_bytes32_to_hex_v1(&s);
    let tokens = an1_derive_input_tokens_v1(&s, job_tokens)?;
    let program = an1_compile_program_v1(tokens.len())?;
    let (values, outputs) = an1_execute_program_v1(&program, &tokens, weights)?;
    let trace_records: Vec<Vec<u8>> = values.iter().enumerate().map(|(i, &v)| an1_encode_iv_record_v1(i, v)).collect();
    let output_records: Vec<Vec<u8>> = outputs.iter().enumerate().map(|(i, &v)| an1_encode_iv_record_v1(i, v)).collect();
    let input_records: Vec<Vec<u8>> = tokens.iter().enumerate().map(|(i, &v)| an1_encode_iv_record_v1(i, v)).collect();
    let rx = An1TreeV1::build("input", &s_hex, &input_records, None)?.root_hex();
    let rt = An1TreeV1::build("trace", &s_hex, &trace_records, None)?.root_hex();
    let ry = An1TreeV1::build("output", &s_hex, &output_records, None)?.root_hex();
    let claim = An1ClaimV1 {
        slot: context.slot_id_hex()?,
        owner: context.owner.clone(),
        attempt: s_hex,
        input_root: rx,
        output_root: ry,
        trace_root: rt,
        program_root: program.tree.root_hex(),
        trace_count: trace_records.len() as u64,
        output_count: 2,
    };
    let material = An1MaterialV1 {
        weights: weights.to_vec(),
        trace_records,
        output_records,
        trace_padding: BTreeMap::new(),
        claimed_model_root: None,
        claimed_trace_root: None,
    };
    Ok((claim, material))
}

/// Authenticated native context (`NativeContext`, `RFC-0059 §7`, `an1_reference.py::NativeContext`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct An1NativeContextV1 {
    pub registration: An1RegistrationContextV1,
    pub sigma: [u8; 32],
    pub job_tokens: Vec<i64>,
    pub claim: An1ClaimV1,
    pub program: An1ProgramV1,
    pub input_tree: An1TreeV1,
}

/// Model-free Node local Court context (`CourtContext`, `RFC-0059 §8.1`, `an1_reference.py::CourtContext`).
///
/// Contains NO full model weight arrays, NO full program instruction arrays, and NO full trace arrays.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct An1CourtContextV1 {
    pub model_root: [u8; 32],
    pub claim: An1ClaimV1,
    pub program_scope: String,
    pub input_length: usize,
    pub output_refs: Vec<usize>,
}

impl An1NativeContextV1 {
    pub fn build(
        context: &An1RegistrationContextV1,
        sigma: &[u8; 32],
        job_tokens: &[i64],
        claim: &An1ClaimV1,
    ) -> Result<Self, An1ErrorV1> {
        claim.validate()?;
        let s = context.attempt_id(sigma)?;
        let s_hex = an1_bytes32_to_hex_v1(&s);
        if an1_job_tree_v1(job_tokens)?.root_hex() != context.job_root {
            return Err(An1ErrorV1::JobRootMismatch);
        }
        let tokens = an1_derive_input_tokens_v1(&s, job_tokens)?;
        let program = an1_compile_program_v1(tokens.len())?;
        let input_records: Vec<Vec<u8>> = tokens.iter().enumerate().map(|(i, &t)| an1_encode_iv_record_v1(i, t)).collect();
        let input_tree = An1TreeV1::build("input", &s_hex, &input_records, None)?;
        if claim.slot != context.slot_id_hex()?
            || claim.owner != context.owner
            || claim.attempt != s_hex
            || claim.input_root != input_tree.root_hex()
            || claim.program_root != program.tree.root_hex()
            || claim.trace_count != program.instructions.len() as u64
            || claim.output_count != 2
        {
            return Err(An1ErrorV1::ClaimDoesNotMatchNativeRegisteredStatement);
        }
        Ok(Self {
            registration: context.clone(),
            sigma: *sigma,
            job_tokens: job_tokens.to_vec(),
            claim: claim.clone(),
            program,
            input_tree,
        })
    }

    pub fn court_view(&self) -> Result<An1CourtContextV1, An1ErrorV1> {
        Ok(An1CourtContextV1 {
            model_root: an1_hex32_to_bytes_v1(&self.registration.model_root)?,
            claim: self.claim.clone(),
            program_scope: self.program.tree.scope.clone(),
            input_length: self.program.input_length,
            output_refs: self.program.output_refs.clone(),
        })
    }
}

/// Local Court evidence types (`RFC-0059 §8.1`, `an1_reference.py`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum An1CourtEvidenceKindV1 {
    Step {
        program: An1OpeningV1,
        target: An1OpeningV1,
        parents: Vec<An1OpeningV1>,
        weight: Option<An1OpeningV1>,
        input: Option<An1OpeningV1>,
    },
    Output {
        output: An1OpeningV1,
        trace: An1OpeningV1,
    },
    Padding {
        target: An1OpeningV1,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct An1CourtEvidenceV1 {
    pub original_id: String,
    pub index: usize,
    pub kind: An1CourtEvidenceKindV1,
}

/// Verdict returned by the model-free Node local Court (`RFC-0059 §8.2`, `RFC-0057 §17.4`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum An1CourtVerdictV1 {
    InvalidEvidence,
    NoRefutation,
    ComputationRefuted,
}

/// Build step evidence at `index` (`an1_reference.py::make_step_evidence`).
pub fn an1_make_step_evidence_v1(
    native: &An1NativeContextV1,
    material: &An1MaterialV1,
    index: usize,
) -> Result<An1CourtEvidenceV1, An1ErrorV1> {
    let model = an1_model_tree_v1(&material.weights)?;
    let trace = An1TreeV1::build("trace", &native.claim.attempt, &material.trace_records, Some(&material.trace_padding))?;
    let instruction = &native.program.instructions[index];
    let mut parents = Vec::with_capacity(instruction.args.len());
    for &arg_idx in &instruction.args {
        parents.push(trace.opening(arg_idx)?);
    }
    let mut weight = None;
    let parsed_parents: Result<Vec<i64>, _> = parents.iter().map(|p| an1_parse_iv_record_v1(&p.payload, p.index)).collect();
    if let Ok(pvals) = parsed_parents
        && let Ok(Some(w_idx)) = an1_expected_weight_index_v1(instruction, &pvals)
    {
        weight = Some(model.opening(w_idx)?);
    }
    let input =
        if instruction.op == An1InstructionOpV1::Input { Some(native.input_tree.opening(instruction.imm[0] as usize)?) } else { None };
    Ok(An1CourtEvidenceV1 {
        original_id: native.claim.original_id_hex()?,
        index,
        kind: An1CourtEvidenceKindV1::Step {
            program: native.program.tree.opening(index)?,
            target: trace.opening(index)?,
            parents,
            weight,
            input,
        },
    })
}

/// Build output evidence at `index` (`an1_reference.py::make_output_evidence`).
pub fn an1_make_output_evidence_v1(
    native: &An1NativeContextV1,
    material: &An1MaterialV1,
    index: usize,
) -> Result<An1CourtEvidenceV1, An1ErrorV1> {
    let trace = An1TreeV1::build("trace", &native.claim.attempt, &material.trace_records, Some(&material.trace_padding))?;
    let output = An1TreeV1::build("output", &native.claim.attempt, &material.output_records, None)?;
    Ok(An1CourtEvidenceV1 {
        original_id: native.claim.original_id_hex()?,
        index,
        kind: An1CourtEvidenceKindV1::Output {
            output: output.opening(index)?,
            trace: trace.opening(native.program.output_refs[index])?,
        },
    })
}

/// Build padding evidence at `index` (`an1_reference.py::make_padding_evidence`).
pub fn an1_make_padding_evidence_v1(
    native: &An1NativeContextV1,
    material: &An1MaterialV1,
    index: usize,
) -> Result<An1CourtEvidenceV1, An1ErrorV1> {
    let trace = An1TreeV1::build("trace", &native.claim.attempt, &material.trace_records, Some(&material.trace_padding))?;
    Ok(An1CourtEvidenceV1 {
        original_id: native.claim.original_id_hex()?,
        index,
        kind: An1CourtEvidenceKindV1::Padding { target: trace.opening(index)? },
    })
}

/// Model-free Node local Court checker (`RFC-0059 §8`, `RFC-0057 §17.3–§17.4`, `an1_reference.py::judge_court`).
pub fn an1_judge_court_v1(ctx: &An1CourtContextV1, evidence: &An1CourtEvidenceV1) -> An1CourtVerdictV1 {
    let Ok(expected_orig_id) = ctx.claim.original_id_hex() else {
        return An1CourtVerdictV1::InvalidEvidence;
    };
    if evidence.original_id != expected_orig_id {
        return An1CourtVerdictV1::InvalidEvidence;
    }
    let Ok(trace_root) = an1_hex32_to_bytes_v1(&ctx.claim.trace_root) else {
        return An1CourtVerdictV1::InvalidEvidence;
    };
    let Ok(output_root) = an1_hex32_to_bytes_v1(&ctx.claim.output_root) else {
        return An1CourtVerdictV1::InvalidEvidence;
    };
    let Ok(program_root) = an1_hex32_to_bytes_v1(&ctx.claim.program_root) else {
        return An1CourtVerdictV1::InvalidEvidence;
    };
    let Ok(input_root) = an1_hex32_to_bytes_v1(&ctx.claim.input_root) else {
        return An1CourtVerdictV1::InvalidEvidence;
    };
    let trace_n = ctx.claim.trace_count as usize;
    let output_n = ctx.claim.output_count as usize;
    let s = &ctx.claim.attempt;
    let i = evidence.index;

    match &evidence.kind {
        An1CourtEvidenceKindV1::Padding { target } => {
            let l = if trace_n <= 1 { 1 } else { 1usize << (usize::BITS - (trace_n - 1).leading_zeros()) };
            if i < trace_n || i >= l {
                return An1CourtVerdictV1::InvalidEvidence;
            }
            if target.index != i || !an1_verify_opening_v1("trace", s, trace_n, &trace_root, target) {
                return An1CourtVerdictV1::InvalidEvidence;
            }
            if target.payload == an1_pad_record_v1(i) {
                An1CourtVerdictV1::NoRefutation
            } else {
                An1CourtVerdictV1::ComputationRefuted
            }
        }
        An1CourtEvidenceKindV1::Output { output, trace } => {
            if i >= output_n || i >= ctx.output_refs.len() {
                return An1CourtVerdictV1::InvalidEvidence;
            }
            let expected_ref = ctx.output_refs[i];
            if output.index != i
                || trace.index != expected_ref
                || !an1_verify_opening_v1("output", s, output_n, &output_root, output)
                || !an1_verify_opening_v1("trace", s, trace_n, &trace_root, trace)
            {
                return An1CourtVerdictV1::InvalidEvidence;
            }
            let (Ok(outv), Ok(tracev)) =
                (an1_parse_iv_record_v1(&output.payload, i), an1_parse_iv_record_v1(&trace.payload, expected_ref))
            else {
                return An1CourtVerdictV1::ComputationRefuted;
            };
            if outv < 0 || (outv as usize) >= AN1_VOCABULARY_V1 {
                return An1CourtVerdictV1::ComputationRefuted;
            }
            if outv == tracev { An1CourtVerdictV1::NoRefutation } else { An1CourtVerdictV1::ComputationRefuted }
        }
        An1CourtEvidenceKindV1::Step { program, target, parents, weight, input } => {
            if i >= trace_n {
                return An1CourtVerdictV1::InvalidEvidence;
            }
            if program.index != i
                || target.index != i
                || !an1_verify_opening_v1("program", &ctx.program_scope, trace_n, &program_root, program)
                || !an1_verify_opening_v1("trace", s, trace_n, &trace_root, target)
            {
                return An1CourtVerdictV1::InvalidEvidence;
            }
            let Ok(instruction) = An1InstructionV1::parse_and_validate(&program.payload, i, ctx.input_length) else {
                return An1CourtVerdictV1::InvalidEvidence;
            };
            if parents.len() != instruction.args.len() {
                return An1CourtVerdictV1::InvalidEvidence;
            }
            for (o, &expected_arg) in parents.iter().zip(instruction.args.iter()) {
                if o.index != expected_arg || !an1_verify_opening_v1("trace", s, trace_n, &trace_root, o) {
                    return An1CourtVerdictV1::InvalidEvidence;
                }
            }
            let Ok(recorded) = an1_parse_iv_record_v1(&target.payload, i) else {
                return An1CourtVerdictV1::ComputationRefuted;
            };
            let mut parent_values = Vec::with_capacity(parents.len());
            for o in parents {
                let Ok(v) = an1_parse_iv_record_v1(&o.payload, o.index) else {
                    return An1CourtVerdictV1::ComputationRefuted;
                };
                parent_values.push(v);
            }
            let Ok(weight_index) = an1_expected_weight_index_v1(&instruction, &parent_values) else {
                return An1CourtVerdictV1::ComputationRefuted;
            };
            let mut weight_val = None;
            if let Some(wi) = weight_index {
                let Some(w_open) = weight else {
                    return An1CourtVerdictV1::InvalidEvidence;
                };
                if w_open.index != wi
                    || !an1_verify_opening_v1("model", &an1_profile_id_hex_v1(), AN1_WEIGHT_COUNT_V1, &ctx.model_root, w_open)
                {
                    return An1CourtVerdictV1::InvalidEvidence;
                }
                let Ok(wv) = an1_parse_iv_record_v1(&w_open.payload, wi) else {
                    return An1CourtVerdictV1::ComputationRefuted;
                };
                weight_val = Some(wv);
            }
            let mut token_val = None;
            if instruction.op == An1InstructionOpV1::Input {
                let expected_idx = instruction.imm[0] as usize;
                let Some(in_open) = input else {
                    return An1CourtVerdictV1::InvalidEvidence;
                };
                if in_open.index != expected_idx || !an1_verify_opening_v1("input", s, ctx.input_length, &input_root, in_open) {
                    return An1CourtVerdictV1::InvalidEvidence;
                }
                let Ok(tv) = an1_parse_iv_record_v1(&in_open.payload, expected_idx) else {
                    return An1CourtVerdictV1::ComputationRefuted;
                };
                token_val = Some(tv);
            }
            let Ok(expected) = an1_evaluate_instruction_v1(&instruction, &parent_values, token_val, weight_val) else {
                return An1CourtVerdictV1::ComputationRefuted;
            };
            if recorded == expected { An1CourtVerdictV1::NoRefutation } else { An1CourtVerdictV1::ComputationRefuted }
        }
    }
}

/// Scope descriptor for a material/structure hold (`RFC-0059 §8.2`, `RFC-0057 §17.4`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct An1RequiredScopeV1 {
    pub kind: String,
    pub index: usize,
}

impl An1RequiredScopeV1 {
    pub fn encode_canonical_json(&self) -> Vec<u8> {
        format!(r#"{{"index":{},"kind":"{}"}}"#, self.index, self.kind).into_bytes()
    }
}

/// Outcome of full Panel inspection (`an1_reference.py::panel_review`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum An1PanelReviewOutcomeV1 {
    ApprovedFullReplay { checked_records: usize, output_tokens: Vec<i64> },
    Refuted { original_id: String, court: An1CourtEvidenceV1 },
    MaterialProblem { status: String, original_id: String, reason: String, required_scope: An1RequiredScopeV1 },
}

/// Full Panel replay and byte-level root authentication (`RFC-0059 §7–§9`, `an1_reference.py::panel_review`).
///
/// - Never trusts `material.claimed_model_root` or `material.claimed_trace_root`.
/// - Routes oversized (`> 512` B) or unserviceable records to `MaterialProblem` with `required_scope`
///   so the selected Panel files a qualified `material_notice` hold (`AN1-16`, `A57-11`).
/// - Otherwise replays the entire execution graph and returns either `ApprovedFullReplay` or
///   a verified `ComputationRefuted` Court witness at the first divergence.
pub fn an1_panel_review_v1(native: &An1NativeContextV1, material: Option<&An1MaterialV1>) -> An1PanelReviewOutcomeV1 {
    let orig_id = native.claim.original_id_hex().unwrap_or_default();
    let problem = |status: &str, reason: &str, kind: &str, index: usize| An1PanelReviewOutcomeV1::MaterialProblem {
        status: status.to_owned(),
        original_id: orig_id.clone(),
        reason: reason.to_owned(),
        required_scope: An1RequiredScopeV1 { kind: kind.to_owned(), index },
    };
    let Some(mat) = material else {
        return problem("MaterialsUnavailable", "required material was not supplied", "trace", 0);
    };
    if mat.weights.len() != AN1_WEIGHT_COUNT_V1 {
        return problem("MaterialsUnauthenticated", "model weight array shape", "model", 0);
    }
    let trace_n = native.claim.trace_count as usize;
    if mat.trace_records.len() != trace_n {
        return problem("MaterialsUnauthenticated", "trace shape is not the committed profile shape", "trace", 0);
    }
    for (i, raw) in mat.trace_records.iter().enumerate() {
        if raw.len() > AN1_MAX_LEAF_BYTES_V1 {
            return problem("MaterialsUnauthenticated", "trace record cannot enter bounded Court", "trace", i);
        }
    }
    let trace_l = if trace_n <= 1 { 1 } else { 1usize << (usize::BITS - (trace_n - 1).leading_zeros()) };
    for (&i, raw) in &mat.trace_padding {
        if i < trace_n || i >= trace_l || raw.len() > AN1_MAX_LEAF_BYTES_V1 {
            return problem("MaterialsUnauthenticated", "padding cannot enter bounded Court", "trace", 0);
        }
    }
    if mat.output_records.len() != 2 {
        return problem("MaterialsUnauthenticated", "output shape", "output", 0);
    }
    for (i, raw) in mat.output_records.iter().enumerate() {
        if raw.len() > AN1_MAX_LEAF_BYTES_V1 {
            return problem("MaterialsUnauthenticated", "output record cannot enter bounded Court", "output", i);
        }
    }

    let Ok(court_view) = native.court_view() else {
        return problem("MaterialsUnauthenticated", "malformed native court view", "trace", 0);
    };
    let refutation_or_hold = |ev_res: Result<An1CourtEvidenceV1, An1ErrorV1>| -> An1PanelReviewOutcomeV1 {
        if let Ok(court) = ev_res
            && an1_judge_court_v1(&court_view, &court) == An1CourtVerdictV1::ComputationRefuted
        {
            return An1PanelReviewOutcomeV1::Refuted { original_id: orig_id.clone(), court };
        }
        problem("MaterialsUnavailable", "bounded local evidence could not be established", "trace", 0)
    };

    let Ok(model) = an1_model_tree_v1(&mat.weights) else {
        return problem("MaterialsUnauthenticated", "malformed model weights", "model", 0);
    };
    let Ok(trace) = An1TreeV1::build("trace", &native.claim.attempt, &mat.trace_records, Some(&mat.trace_padding)) else {
        return problem("MaterialsUnauthenticated", "malformed trace tree", "trace", 0);
    };
    let Ok(output) = An1TreeV1::build("output", &native.claim.attempt, &mat.output_records, None) else {
        return problem("MaterialsUnauthenticated", "malformed output tree", "output", 0);
    };

    if model.root_hex() != native.registration.model_root {
        return problem("MaterialsUnauthenticated", "model bytes do not match registered root", "model", 0);
    }
    if trace.root_hex() != native.claim.trace_root || trace.n != trace_n {
        return problem("MaterialsUnauthenticated", "trace bytes do not match original root", "trace", 0);
    }
    if output.root_hex() != native.claim.output_root || output.n != 2 {
        return problem("MaterialsUnauthenticated", "output bytes do not match original root", "output", 0);
    }

    for i in trace.n..trace.l {
        if trace.records[i] != an1_pad_record_v1(i) {
            return refutation_or_hold(an1_make_padding_evidence_v1(native, mat, i));
        }
    }

    let Ok(s) = an1_hex32_to_bytes_v1(&native.claim.attempt) else {
        return problem("MaterialsUnauthenticated", "invalid attempt hex", "trace", 0);
    };
    let Ok(tokens) = an1_derive_input_tokens_v1(&s, &native.job_tokens) else {
        return problem("MaterialsUnauthenticated", "invalid input derivation", "input", 0);
    };
    let Ok((values, outputs)) = an1_execute_program_v1(&native.program, &tokens, &mat.weights) else {
        return problem("MaterialsUnauthenticated", "execution error", "trace", 0);
    };

    for (i, &val) in values.iter().enumerate() {
        if mat.trace_records[i] != an1_encode_iv_record_v1(i, val) {
            return refutation_or_hold(an1_make_step_evidence_v1(native, mat, i));
        }
    }
    for (i, &val) in outputs.iter().enumerate() {
        if mat.output_records[i] != an1_encode_iv_record_v1(i, val) {
            return refutation_or_hold(an1_make_output_evidence_v1(native, mat, i));
        }
    }

    An1PanelReviewOutcomeV1::ApprovedFullReplay { checked_records: values.len(), output_tokens: outputs }
}

/// Per-slot state row in `An1LedgerStateV1` (`an1_reference.py::Ledger`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct An1SlotRowV1 {
    pub context: An1RegistrationContextV1,
    pub job_tokens: Vec<i64>,
    pub funding_ref: String,
    pub claim: Option<An1ClaimV1>,
    pub panel: Option<Vec<String>>,
    pub pending_material_cases: BTreeSet<String>,
    pub refuted: bool,
    pub active: bool,
    pub approved: BTreeSet<String>,
    pub materials_ready: bool,
    pub closed: bool,
    pub delivered: bool,
    pub is_final: bool,
}

/// Undo journal entry for `rollback_last_final` (`an1_reference.py::Ledger.rollback_last_final`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct An1FinalUndoEntryV1 {
    pub slot: String,
    pub old_credit: Option<u64>,
    pub old_final: bool,
    pub attempt: String,
    pub work_key: String,
    pub useful_key: Option<String>,
}

/// Result of `apply_batch` (`RFC-0059 §10`, `an1_reference.py::Ledger.apply_batch`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct An1BatchOutcomeV1 {
    pub material_holds: Vec<String>,
    pub court_results: Vec<An1CourtVerdictV1>,
    pub is_final: bool,
}

/// Complete `PALW-AN1` state machine & atomic Final ledger (`RFC-0059 §4, §7–§10`, `an1_reference.py::Ledger`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct An1LedgerStateV1 {
    pub chain_id: String,
    /// `(beacon_key, beacon_round) -> (sigma_bytes32, earliest_access_time)`
    pub beacons: BTreeMap<(String, u64), ([u8; 32], u64)>,
    /// `panel_event -> later_seed_bytes32`
    pub panel_events: BTreeMap<u64, [u8; 32]>,
    pub rows: BTreeMap<String, An1SlotRowV1>,
    pub registration_slots: BTreeMap<String, String>,
    pub consumed_slots: BTreeSet<String>,
    pub attempt_consumed: BTreeSet<String>,
    pub work_consumed: BTreeSet<String>,
    pub useful_consumed: BTreeSet<String>,
    pub credits: BTreeMap<String, u64>,
    pub journal: Vec<An1FinalUndoEntryV1>,
}

impl Default for An1LedgerStateV1 {
    fn default() -> Self {
        let mut beacons = BTreeMap::new();
        beacons.insert(("fixture-key-1".to_owned(), 13), ([0x33u8; 32], 20));
        Self::new(beacons, None, "an1-independent-test-chain")
    }
}

impl An1LedgerStateV1 {
    pub fn new(
        beacons: BTreeMap<(String, u64), ([u8; 32], u64)>,
        panel_events: Option<BTreeMap<u64, [u8; 32]>>,
        chain_id: &str,
    ) -> Self {
        let panel_events = panel_events.unwrap_or_else(|| {
            let mut m = BTreeMap::new();
            m.insert(100, [0x44u8; 32]);
            m
        });
        Self {
            chain_id: chain_id.to_owned(),
            beacons,
            panel_events,
            rows: BTreeMap::new(),
            registration_slots: BTreeMap::new(),
            consumed_slots: BTreeSet::new(),
            attempt_consumed: BTreeSet::new(),
            work_consumed: BTreeSet::new(),
            useful_consumed: BTreeSet::new(),
            credits: BTreeMap::new(),
            journal: Vec::new(),
        }
    }

    /// Freeze 1: Register `context` and `job_tokens` strictly before execution beacon access (`RFC-0059 §2.3, §4`).
    pub fn register(
        &mut self,
        context: &An1RegistrationContextV1,
        job_tokens: &[i64],
        now: u64,
        authenticated_owner: &str,
        funding_ref: &str,
    ) -> Result<String, An1ErrorV1> {
        context.validate()?;
        let key = (context.beacon_key.clone(), context.beacon_round);
        let &(_, access_time) = self.beacons.get(&key).ok_or(An1ErrorV1::RegistrationMustPrecedeBeaconAccess)?;
        if now >= access_time
            || authenticated_owner != context.owner
            || context.chain != self.chain_id
            || an1_job_tree_v1(job_tokens)?.root_hex() != context.job_root
            || !(access_time < context.panel_event && context.panel_event < context.deadline)
        {
            return Err(An1ErrorV1::RegistrationMustPrecedeBeaconAccess);
        }
        let slot = context.slot_id_hex()?;
        let reg_key = an1_bytes32_to_hex_v1(&context.registration_key()?);
        if self.rows.contains_key(&slot) || self.registration_slots.contains_key(&reg_key) {
            return Err(An1ErrorV1::DuplicateRegistrationOrSlot);
        }
        self.registration_slots.insert(reg_key, slot.clone());
        self.rows.insert(
            slot.clone(),
            An1SlotRowV1 {
                context: context.clone(),
                job_tokens: job_tokens.to_vec(),
                funding_ref: funding_ref.to_owned(),
                claim: None,
                panel: None,
                pending_material_cases: BTreeSet::new(),
                refuted: false,
                active: true,
                approved: BTreeSet::new(),
                materials_ready: false,
                closed: false,
                delivered: false,
                is_final: false,
            },
        );
        Ok(slot)
    }

    /// Freeze 2: Seal the original `claim` once in `[beacon_access_time, panel_event)` (`RFC-0059 §4, §7`).
    pub fn seal(&mut self, slot: &str, claim: &An1ClaimV1, now: u64, authenticated_owner: &str) -> Result<(), An1ErrorV1> {
        claim.validate()?;
        let row = self.rows.get(slot).ok_or(An1ErrorV1::OriginalSealOwnerTimingOrTicketInvalid)?;
        let c = &row.context;
        let &(sigma, access_time) =
            self.beacons.get(&(c.beacon_key.clone(), c.beacon_round)).ok_or(An1ErrorV1::OriginalSealOwnerTimingOrTicketInvalid)?;
        let ticket_be = claim.ticket_bytes()?;
        if row.claim.is_some()
            || authenticated_owner != c.owner
            || !(access_time <= now && now < c.panel_event)
            || !c.target.admits_ticket_be(&ticket_be)
        {
            return Err(An1ErrorV1::OriginalSealOwnerTimingOrTicketInvalid);
        }
        let _ = An1NativeContextV1::build(c, &sigma, &row.job_tokens, claim)?;
        self.rows.get_mut(slot).unwrap().claim = Some(claim.clone());
        Ok(())
    }

    /// Select the 5-member Panel after `panel_event` using the dedicated post-seal Panel event seed (`RFC-0059 §4`).
    pub fn select_panel(
        &mut self,
        slot: &str,
        later_seed: &[u8; 32],
        now: u64,
        candidates: &[String],
    ) -> Result<Vec<String>, An1ErrorV1> {
        for actor in candidates {
            if actor.is_empty() || actor.len() > 64 || !actor.is_ascii() {
                return Err(An1ErrorV1::ClaimMustBeSealedBeforeUniqueLaterPanelEvent);
            }
        }
        let unique_candidates: BTreeSet<String> = candidates.iter().cloned().collect();
        let row = self.rows.get(slot).ok_or(An1ErrorV1::ClaimMustBeSealedBeforeUniqueLaterPanelEvent)?;
        let Some(claim) = &row.claim else {
            return Err(An1ErrorV1::ClaimMustBeSealedBeforeUniqueLaterPanelEvent);
        };
        if row.panel.is_some()
            || now < row.context.panel_event
            || unique_candidates.len() < 5
            || self.panel_events.get(&row.context.panel_event) != Some(later_seed)
        {
            return Err(An1ErrorV1::ClaimMustBeSealedBeforeUniqueLaterPanelEvent);
        }
        let orig_id_bytes = claim.original_id_bytes()?;
        let mut ranked: Vec<([u8; 32], String)> = unique_candidates
            .into_iter()
            .map(|actor| {
                let draw = an1_h256_v1("AN1-panel-draw", &[later_seed, &orig_id_bytes, actor.as_bytes()]);
                (draw, actor)
            })
            .collect();
        ranked.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let selected: Vec<String> = ranked.into_iter().take(5).map(|(_, actor)| actor).collect();
        self.rows.get_mut(slot).unwrap().panel = Some(selected.clone());
        Ok(selected)
    }

    /// Reconstruct `An1NativeContextV1` from current state (`an1_reference.py::Ledger.native`).
    pub fn native(&self, slot: &str) -> Result<An1NativeContextV1, An1ErrorV1> {
        let row = self.rows.get(slot).ok_or(An1ErrorV1::ClaimDoesNotMatchNativeRegisteredStatement)?;
        let claim = row.claim.as_ref().ok_or(An1ErrorV1::ClaimDoesNotMatchNativeRegisteredStatement)?;
        let &(sigma, _) = self
            .beacons
            .get(&(row.context.beacon_key.clone(), row.context.beacon_round))
            .ok_or(An1ErrorV1::ClaimDoesNotMatchNativeRegisteredStatement)?;
        An1NativeContextV1::build(&row.context, &sigma, &row.job_tokens, claim)
    }

    /// Record a positive Panel approval from a selected Panel member (`an1_reference.py::Ledger.approve`).
    pub fn approve(&mut self, slot: &str, authenticated_actor: &str) -> Result<(), An1ErrorV1> {
        let row = self.rows.get_mut(slot).ok_or(An1ErrorV1::NotSelectedPanelActor)?;
        let Some(panel) = &row.panel else {
            return Err(An1ErrorV1::NotSelectedPanelActor);
        };
        if !panel.iter().any(|p| p == authenticated_actor) {
            return Err(An1ErrorV1::NotSelectedPanelActor);
        }
        row.approved.insert(authenticated_actor.to_owned());
        Ok(())
    }

    /// Evaluate a local Court witness and mark the slot refuted on `ComputationRefuted` (`an1_reference.py::Ledger.court`).
    pub fn court(&mut self, slot: &str, evidence: &An1CourtEvidenceV1) -> Result<An1CourtVerdictV1, An1ErrorV1> {
        let native = self.native(slot)?;
        let view = native.court_view()?;
        let verdict = an1_judge_court_v1(&view, evidence);
        if verdict == An1CourtVerdictV1::ComputationRefuted {
            self.rows.get_mut(slot).unwrap().refuted = true;
        }
        Ok(verdict)
    }

    /// Record a qualified material notice from a selected Panel member, adding a hold in `pending_material_cases` (`RFC-0059 §8.2, §10`, `AN1-16`).
    pub fn material_notice(
        &mut self,
        slot: &str,
        authenticated_actor: &str,
        review_outcome: &An1PanelReviewOutcomeV1,
        now: u64,
    ) -> Result<String, An1ErrorV1> {
        let case_id = self.material_case_id(slot, authenticated_actor, review_outcome, now)?;
        self.rows.get_mut(slot).unwrap().pending_material_cases.insert(case_id.clone());
        Ok(case_id)
    }

    fn material_case_id(
        &self,
        slot: &str,
        authenticated_actor: &str,
        review_outcome: &An1PanelReviewOutcomeV1,
        now: u64,
    ) -> Result<String, An1ErrorV1> {
        let row = self.rows.get(slot).ok_or_else(|| An1ErrorV1::UnqualifiedMaterialNotice("unknown slot".to_owned()))?;
        let Some(claim) = &row.claim else {
            return Err(An1ErrorV1::UnqualifiedMaterialNotice("unsealed slot".to_owned()));
        };
        let Some(panel) = &row.panel else {
            return Err(An1ErrorV1::UnqualifiedMaterialNotice("panel not selected".to_owned()));
        };
        if !panel.iter().any(|p| p == authenticated_actor)
            || row.is_final
            || !(row.context.panel_event <= now && now <= row.context.deadline)
        {
            return Err(An1ErrorV1::UnqualifiedMaterialNotice("actor, final state, or timing invalid".to_owned()));
        }
        let An1PanelReviewOutcomeV1::MaterialProblem { status, original_id, required_scope, .. } = review_outcome else {
            return Err(An1ErrorV1::UnqualifiedMaterialNotice("not a material problem report".to_owned()));
        };
        if *original_id != claim.original_id_hex()? || (status != "MaterialsUnavailable" && status != "MaterialsUnauthenticated") {
            return Err(An1ErrorV1::UnqualifiedMaterialNotice("original_id or status mismatch".to_owned()));
        }
        let trace_n = claim.trace_count as usize;
        let trace_l = if trace_n <= 1 { 1 } else { 1usize << (usize::BITS - (trace_n - 1).leading_zeros()) };
        let limit = match required_scope.kind.as_str() {
            "trace" => trace_l,
            "output" => 2,
            "model" => AN1_WEIGHT_COUNT_V1,
            "input" => 35 + row.job_tokens.len(),
            "program" => trace_n,
            _ => return Err(An1ErrorV1::MaterialScopeOutsideAuthenticatedClaim),
        };
        if required_scope.index >= limit {
            return Err(An1ErrorV1::MaterialScopeOutsideAuthenticatedClaim);
        }
        let orig_bytes = claim.original_id_bytes()?;
        let scope_json = required_scope.encode_canonical_json();
        let case_id =
            an1_bytes32_to_hex_v1(&an1_h256_v1("AN1-material-case", &[&orig_bytes, authenticated_actor.as_bytes(), &scope_json]));
        Ok(case_id)
    }

    /// Atomic Final transition (`RFC-0059 §10`, `an1_reference.py::Ledger.finalize`).
    ///
    /// Re-verifies current `NativeContext`, checks all holds/refutations/deadlines/approvals,
    /// verifies `slot`, `attempt_id`, `ComputationalWorkKey`, and (when `"AN1-TEST-USEFUL-ONCE-1"`)
    /// `BaseWorkKey` are unconsumed, and commits all consumption flags + credit atomically.
    pub fn finalize(&mut self, slot: &str, now: u64) -> bool {
        let Some(row) = self.rows.get(slot).cloned() else {
            return false;
        };
        let Some(claim) = &row.claim else {
            return false;
        };
        if row.context.validate().is_err() || claim.validate().is_err() {
            return false;
        }
        let Some(&(sigma, _)) = self.beacons.get(&(row.context.beacon_key.clone(), row.context.beacon_round)) else {
            return false;
        };
        let Ok(work_key_bytes) = an1_computational_work_key_v1(&row.context, &sigma, &row.job_tokens) else {
            return false;
        };
        let work_key = an1_bytes32_to_hex_v1(&work_key_bytes);
        let useful_key = if row.context.accounting_version == "AN1-TEST-USEFUL-ONCE-1" {
            let Ok(uk) = an1_base_work_key_v1(&row.context) else {
                return false;
            };
            Some(an1_bytes32_to_hex_v1(&uk))
        } else {
            None
        };
        let Ok(expected_slot) = row.context.slot_id_hex() else {
            return false;
        };
        let Ok(ticket_be) = claim.ticket_bytes() else {
            return false;
        };
        let Some(panel) = &row.panel else {
            return false;
        };
        let panel_set: BTreeSet<String> = panel.iter().cloned().collect();
        if self.consumed_slots.contains(slot)
            || row.is_final
            || slot != expected_slot
            || claim.slot != slot
            || self.attempt_consumed.contains(&claim.attempt)
            || self.work_consumed.contains(&work_key)
            || useful_key.as_ref().is_some_and(|uk| self.useful_consumed.contains(uk))
            || !row.active
            || !(row.context.panel_event <= now && now <= row.context.deadline)
            || panel.len() != 5
            || !row.pending_material_cases.is_empty()
            || row.refuted
            || row.approved.is_empty()
            || !row.approved.is_subset(&panel_set)
            || !row.materials_ready
            || !row.closed
            || !row.delivered
            || !row.context.target.admits_ticket_be(&ticket_be)
        {
            return false;
        }
        if self.native(slot).is_err() {
            return false;
        }
        let old_credit = self.credits.get(&row.context.owner).copied();
        let Some(new_credit) = old_credit.unwrap_or(0).checked_add(1) else {
            return false;
        };
        let undo = An1FinalUndoEntryV1 {
            slot: slot.to_owned(),
            old_credit,
            old_final: row.is_final,
            attempt: claim.attempt.clone(),
            work_key: work_key.clone(),
            useful_key: useful_key.clone(),
        };
        self.consumed_slots.insert(slot.to_owned());
        self.attempt_consumed.insert(claim.attempt.clone());
        self.work_consumed.insert(work_key);
        if let Some(uk) = useful_key {
            self.useful_consumed.insert(uk);
        }
        self.rows.get_mut(slot).unwrap().is_final = true;
        self.credits.insert(row.context.owner.clone(), new_credit);
        self.journal.push(undo);
        true
    }

    /// Execute a canonical batch (`material_notices` -> `court` refutations -> `finalize`, `RFC-0059 §10`).
    pub fn apply_batch(
        &mut self,
        slot: &str,
        evidences: &[An1CourtEvidenceV1],
        request_final: bool,
        now: u64,
        material_notices: &[(String, An1PanelReviewOutcomeV1)],
    ) -> Result<An1BatchOutcomeV1, An1ErrorV1> {
        let material_holds = material_notices
            .iter()
            .map(|(actor, report)| self.material_case_id(slot, actor, report, now))
            .collect::<Result<Vec<_>, _>>()?;
        let court_results = if evidences.is_empty() {
            Vec::new()
        } else {
            let view = self.native(slot)?.court_view()?;
            evidences.iter().map(|evidence| an1_judge_court_v1(&view, evidence)).collect()
        };
        // Apply only after every authenticated input has been checked.
        if let Some(row) = self.rows.get_mut(slot) {
            row.pending_material_cases.extend(material_holds.iter().cloned());
            row.refuted |= court_results.contains(&An1CourtVerdictV1::ComputationRefuted);
        }
        let is_final = if request_final { self.finalize(slot, now) } else { false };
        Ok(An1BatchOutcomeV1 { material_holds, court_results, is_final })
    }

    /// Roll back the last finalized claim during a DAG reorg (`RFC-0059 §10`, `an1_reference.py::Ledger.rollback_last_final`).
    pub fn rollback_last_final(&mut self) -> Option<An1FinalUndoEntryV1> {
        let entry = self.journal.pop()?;
        self.consumed_slots.remove(&entry.slot);
        self.attempt_consumed.remove(&entry.attempt);
        self.work_consumed.remove(&entry.work_key);
        if let Some(uk) = &entry.useful_key {
            self.useful_consumed.remove(uk);
        }
        if let Some(row) = self.rows.get_mut(&entry.slot) {
            row.is_final = entry.old_final;
            if let Some(credit) = entry.old_credit {
                self.credits.insert(row.context.owner.clone(), credit);
            } else {
                self.credits.remove(&row.context.owner);
            }
        }
        Some(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_context() -> (An1RegistrationContextV1, [u8; 32], Vec<i64>, Vec<i64>) {
        let weights = an1_deterministic_weights_v1(23);
        let job_tokens = vec![17, 25];
        let ctx = An1RegistrationContextV1 {
            accounting_version: "AN1-TEST-UNIT-1".to_owned(),
            beacon_key: "fixture-key-1".to_owned(),
            beacon_round: 13,
            chain: "an1-independent-test-chain".to_owned(),
            deadline: 300,
            epoch: 7,
            job_root: an1_job_tree_v1(&job_tokens).unwrap().root_hex(),
            model_root: an1_model_tree_v1(&weights).unwrap().root_hex(),
            nonce: "22".repeat(32),
            owner: "11".repeat(32),
            panel_event: 100,
            profile_id: an1_profile_id_hex_v1(),
            target: An1TargetThreshold257V1::MaxTwoPow256,
        };
        (ctx, [0x33u8; 32], job_tokens, weights)
    }

    /// Hostile tree root builder bypassing `An1TreeV1::build`'s `MAX_LEAF_BYTES` check (`check_an1.py::hostile_tree_root`).
    fn hostile_tree_root(kind: &str, scope: &str, records: &[Vec<u8>]) -> String {
        let n = records.len();
        let l = 1usize << (usize::BITS - (n - 1).leading_zeros());
        let mut level = Vec::with_capacity(l);
        for i in 0..l {
            let raw = if i < n { records[i].clone() } else { an1_pad_record_v1(i) };
            let pre = An1TreeV1::leaf_preimage(kind, scope, n, l, i, &raw);
            let h: [u8; 32] = Sha256::digest(&pre).into();
            level.push(h);
        }
        let mut height = 1usize;
        while level.len() > 1 {
            let mut parents = Vec::with_capacity(level.len() / 2);
            for i in 0..(level.len() / 2) {
                let pre = An1TreeV1::node_preimage(kind, scope, n, l, height, i, &level[2 * i], &level[2 * i + 1]);
                let h: [u8; 32] = Sha256::digest(&pre).into();
                parents.push(h);
            }
            level = parents;
            height += 1;
        }
        an1_bytes32_to_hex_v1(&level[0])
    }

    #[test]
    fn an1_pinned_test_vectors_and_all_acceptance_criteria_an1_01_to_an1_16() {
        // 0. Exact match against pinned test_vectors.json from 2026-11-18-an1
        assert_eq!(an1_profile_id_hex_v1(), AN1_PINNED_PROFILE_ID_HEX_V1);
        let (c, sigma, jobs, weights) = fixture_context();
        assert_eq!(c.model_root, "71fcdbf083856ddb0ec5263ff0370e68a588211a645bf6a4e2f5c40c4a0f2f10");
        assert_eq!(c.job_root, "f49d22a335f588f1ea889c76ba8067468cd393ae82fc1216a2363822d4a5b0f9");
        assert_eq!(c.slot_id_hex().unwrap(), "75fdb7349cfdd8ac04cd722a8f6e0981836762def327db39175bdd81a5bd2b89");
        assert_eq!(c.attempt_id_hex(&sigma).unwrap(), "50a9b39bc0eb7e6bb2503ecd22c4b32d0c0c8a942c8c2872dd04e24e5fcd11eb");

        let (claim, material) = an1_build_claim_v1(&c, &sigma, &jobs, &weights).unwrap();
        let native = An1NativeContextV1::build(&c, &sigma, &jobs, &claim).unwrap();
        assert_eq!(claim.program_root, "82759194b51f7da09ef580e2dc88506348f365be2302bd4d42ad3f7a5b957914");
        assert_eq!(claim.input_root, "3650b271044e63585671f65f249e7a139dc91b6f9c2bbd644416a4216626cd85");
        assert_eq!(claim.trace_root, "7b5ade796e0512376b19eafc9aff83c6878346da938f652621c51c93af9781ff");
        assert_eq!(claim.output_root, "5063037099b4f650ad39224b8238e27615c51d406ffd78b7702ac46d23a21a04");
        assert_eq!(claim.original_id_hex().unwrap(), "912bd3ddc83e5c886e1386fd5493b4dcd0fd854496fc7fdf0a1c507e9448ceb6");
        assert_eq!(claim.ticket_hex().unwrap(), "c0524903d3381beb907e305219ec06d82c82e8898b52ec4f1c948fe72e175745");
        assert_eq!(claim.trace_count, 6153);
        assert_eq!(native.program.output_refs, vec![3078, 6151]);

        // AN1-01 / A56-02: Same C registered under different funding_ref cannot create a second slot
        let mut beacons = BTreeMap::new();
        beacons.insert((c.beacon_key.clone(), c.beacon_round), (sigma, 20));
        let mut ledger = An1LedgerStateV1::new(beacons.clone(), None, &c.chain);
        assert!(ledger.register(&c, &jobs, 20, &c.owner, "f1").is_err()); // at beacon access -> rejected
        let slot = ledger.register(&c, &jobs, 10, &c.owner, "f1").unwrap();
        assert!(ledger.register(&c, &jobs, 11, &c.owner, "different-funding-ref").is_err());

        // AN1-02 & AN1-03: Injective byte-to-token mapping, owner/nonce change invalidates old claim/opening
        let mut alt_c = c.clone();
        alt_c.nonce = "66".repeat(32);
        assert!(An1NativeContextV1::build(&alt_c, &sigma, &jobs, &claim).is_err());
        assert!(an1_derive_input_tokens_v1(&c.attempt_id(&sigma).unwrap(), &[259]).is_err());
        assert!(an1_derive_input_tokens_v1(&c.attempt_id(&sigma).unwrap(), &[1; 9]).is_err());

        // AN1-04: Two freezes — cannot select panel before seal, cannot seal after panel_event, cannot reuse sigma as panel seed
        let candidates: Vec<String> = (0..6).map(|i| format!("p{i}")).collect();
        assert!(ledger.select_panel(&slot, &[0x44; 32], 100, &candidates).is_err());
        assert!(ledger.seal(&slot, &claim, 19, &c.owner).is_err());
        assert!(ledger.seal(&slot, &claim, 100, &c.owner).is_err());
        ledger.seal(&slot, &claim, 30, &c.owner).unwrap();
        assert!(ledger.seal(&slot, &claim, 31, &c.owner).is_err());
        assert!(ledger.select_panel(&slot, &sigma, 100, &candidates).is_err());
        let panel = ledger.select_panel(&slot, &[0x44; 32], 100, &candidates).unwrap();
        assert_eq!(panel.len(), 5);

        // AN1-05 & AN1-06: Model-free Court verifies expected weight index and separates InvalidEvidence vs ComputationRefuted
        let court_view = native.court_view().unwrap();
        let embed_idx = native.program.instructions.iter().position(|ins| ins.op == An1InstructionOpV1::Embed).unwrap();
        let valid_ev = an1_make_step_evidence_v1(&native, &material, embed_idx).unwrap();
        assert_eq!(an1_judge_court_v1(&court_view, &valid_ev), An1CourtVerdictV1::NoRefutation);

        // Wrong weight index from the SAME valid model tree -> InvalidEvidence
        let model_tree = an1_model_tree_v1(&weights).unwrap();
        let mut wrong_wi_ev = valid_ev.clone();
        if let An1CourtEvidenceKindV1::Step { weight: Some(ref mut w_open), .. } = wrong_wi_ev.kind {
            let next_wi = (w_open.index + 1) % AN1_WEIGHT_COUNT_V1;
            *w_open = model_tree.opening(next_wi).unwrap();
        }
        assert_eq!(an1_judge_court_v1(&court_view, &wrong_wi_ev), An1CourtVerdictV1::InvalidEvidence);

        // Authenticated false trace value -> Panel finds Refuted and Node confirms ComputationRefuted
        let mut bad_mat = material.clone();
        let orig_v = an1_parse_iv_record_v1(&bad_mat.trace_records[embed_idx], embed_idx).unwrap();
        bad_mat.trace_records[embed_idx] = an1_encode_iv_record_v1(embed_idx, orig_v + 1);
        let mut bad_claim = claim.clone();
        bad_claim.trace_root = An1TreeV1::build("trace", &claim.attempt, &bad_mat.trace_records, None).unwrap().root_hex();
        let bad_native = An1NativeContextV1::build(&c, &sigma, &jobs, &bad_claim).unwrap();
        let review = an1_panel_review_v1(&bad_native, Some(&bad_mat));
        let An1PanelReviewOutcomeV1::Refuted { court: ref_court, .. } = review else {
            panic!("expected Refuted outcome");
        };
        assert_eq!(an1_judge_court_v1(&bad_native.court_view().unwrap(), &ref_court), An1CourtVerdictV1::ComputationRefuted);

        // AN1-08: 4 dishonest positive approvals vs 1 honest valid refutation in the same batch -> refutation wins
        let mut bad_ledger = An1LedgerStateV1::new(beacons.clone(), None, &c.chain);
        let bad_slot = bad_ledger.register(&c, &jobs, 10, &c.owner, "f1").unwrap();
        bad_ledger.seal(&bad_slot, &bad_claim, 30, &c.owner).unwrap();
        let bad_panel = bad_ledger.select_panel(&bad_slot, &[0x44; 32], 100, &candidates).unwrap();
        for p in &bad_panel[..4] {
            bad_ledger.approve(&bad_slot, p).unwrap();
        }
        let r = bad_ledger.rows.get_mut(&bad_slot).unwrap();
        r.materials_ready = true;
        r.closed = true;
        r.delivered = true;
        let batch_out = bad_ledger.apply_batch(&bad_slot, &[ref_court], true, 180, &[]).unwrap();
        assert_eq!(batch_out.court_results, vec![An1CourtVerdictV1::ComputationRefuted]);
        assert!(!batch_out.is_final);
        assert!(!bad_ledger.consumed_slots.contains(&bad_slot));

        // AN1-15: Float (6153.0) and Bool (true/false) rejected for trace_count / output_count
        assert!(
            An1ClaimV1::from_checked_scalars(
                claim.slot.clone(),
                claim.owner.clone(),
                claim.attempt.clone(),
                claim.input_root.clone(),
                claim.output_root.clone(),
                claim.trace_root.clone(),
                claim.program_root.clone(),
                &An1ScalarValueV1::Float(6153.0),
                &An1ScalarValueV1::Int(2),
            )
            .is_err()
        );
        assert!(
            An1ClaimV1::from_checked_scalars(
                claim.slot.clone(),
                claim.owner.clone(),
                claim.attempt.clone(),
                claim.input_root.clone(),
                claim.output_root.clone(),
                claim.trace_root.clone(),
                claim.program_root.clone(),
                &An1ScalarValueV1::Bool(true),
                &An1ScalarValueV1::Int(2),
            )
            .is_err()
        );

        // AN1-16 / A57-11: Oversized committed leaf (> 512 bytes) hashed via hostile_tree_root produces MaterialProblem
        // and blocks Final via material_notice even with 4 approvals and past deadline!
        let mut huge_mat = material.clone();
        huge_mat.trace_records[0] = vec![b' '; AN1_MAX_LEAF_BYTES_V1 + 1];
        let huge_root = hostile_tree_root("trace", &claim.attempt, &huge_mat.trace_records);
        let mut huge_claim = claim.clone();
        huge_claim.trace_root = huge_root;
        let huge_native = An1NativeContextV1::build(&c, &sigma, &jobs, &huge_claim).unwrap();
        let huge_review = an1_panel_review_v1(&huge_native, Some(&huge_mat));
        assert!(matches!(
            huge_review,
            An1PanelReviewOutcomeV1::MaterialProblem {
                ref status,
                ref required_scope,
                ..
            } if status == "MaterialsUnauthenticated" && required_scope.kind == "trace" && required_scope.index == 0
        ));
        let mut huge_ledger = An1LedgerStateV1::new(beacons.clone(), None, &c.chain);
        let huge_slot = huge_ledger.register(&c, &jobs, 10, &c.owner, "f1").unwrap();
        huge_ledger.seal(&huge_slot, &huge_claim, 30, &c.owner).unwrap();
        let huge_panel = huge_ledger.select_panel(&huge_slot, &[0x44; 32], 100, &candidates).unwrap();
        for p in &huge_panel[..4] {
            huge_ledger.approve(&huge_slot, p).unwrap();
        }
        let hr = huge_ledger.rows.get_mut(&huge_slot).unwrap();
        hr.materials_ready = true;
        hr.closed = true;
        hr.delivered = true;
        let before_bad_batch = huge_ledger.clone();
        let mixed_notices = [(huge_panel[4].clone(), huge_review.clone()), ("unselected".to_owned(), huge_review.clone())];
        assert!(huge_ledger.apply_batch(&huge_slot, &[], true, 180, &mixed_notices).is_err());
        assert_eq!(huge_ledger, before_bad_batch);
        let huge_batch = huge_ledger.apply_batch(&huge_slot, &[], true, 180, &[(huge_panel[4].clone(), huge_review)]).unwrap();
        assert_eq!(huge_batch.material_holds.len(), 1);
        assert!(!huge_batch.is_final);
        assert!(!huge_ledger.finalize(&huge_slot, 301)); // Timeout never turns hold into success!

        // AN1-10 / A56-03: Strict 257-bit target threshold equivalence with CAD1 T(D) = floor((2^256 - 1)/D)
        for d in [1u128, 2, 3, 17, 1 << 20] {
            let t_be = swc1_difficulty_threshold_be_v1(d).unwrap();
            let theta = An1TargetThreshold257V1::from_difficulty_d(d).unwrap();
            assert_eq!(t_be <= t_be, theta.admits_ticket_be(&t_be));
            if d > 1 {
                let mut t_plus_1 = t_be;
                for b in t_plus_1.iter_mut().rev() {
                    if *b < 0xFF {
                        *b += 1;
                        break;
                    }
                    *b = 0;
                }
                assert!(!theta.admits_ticket_be(&t_plus_1));
            }
        }

        // AN1-11 & AN1-12: Atomic Final consumption of slot, attempt, ComputationalWorkKey, BaseWorkKey, and rollback
        let mut u_c1 = c.clone();
        u_c1.accounting_version = "AN1-TEST-USEFUL-ONCE-1".to_owned();
        let mut u_c2 = u_c1.clone();
        u_c2.nonce = "88".repeat(32);
        assert_eq!(an1_base_work_key_v1(&u_c1).unwrap(), an1_base_work_key_v1(&u_c2).unwrap());
        assert_ne!(
            an1_computational_work_key_v1(&u_c1, &sigma, &jobs).unwrap(),
            an1_computational_work_key_v1(&u_c2, &sigma, &jobs).unwrap()
        );
        let (uc1, _) = an1_build_claim_v1(&u_c1, &sigma, &jobs, &weights).unwrap();
        let (uc2, _) = an1_build_claim_v1(&u_c2, &sigma, &jobs, &weights).unwrap();
        let mut u_ledger = An1LedgerStateV1::new(beacons, None, &c.chain);
        let s1 = u_ledger.register(&u_c1, &jobs, 10, &u_c1.owner, "f1").unwrap();
        let s2 = u_ledger.register(&u_c2, &jobs, 10, &u_c2.owner, "f2").unwrap();
        u_ledger.seal(&s1, &uc1, 30, &u_c1.owner).unwrap();
        u_ledger.seal(&s2, &uc2, 30, &u_c2.owner).unwrap();
        let p1 = u_ledger.select_panel(&s1, &[0x44; 32], 100, &candidates).unwrap();
        let p2 = u_ledger.select_panel(&s2, &[0x44; 32], 100, &candidates).unwrap();
        u_ledger.approve(&s1, &p1[0]).unwrap();
        u_ledger.approve(&s2, &p2[0]).unwrap();
        for s in [&s1, &s2] {
            let row = u_ledger.rows.get_mut(s).unwrap();
            row.materials_ready = true;
            row.closed = true;
            row.delivered = true;
        }
        u_ledger.credits.insert(u_c1.owner.clone(), u64::MAX);
        let full = u_ledger.clone();
        assert!(!u_ledger.finalize(&s1, 180));
        assert_eq!(u_ledger, full);
        u_ledger.credits.remove(&u_c1.owner);
        let before_final = u_ledger.clone();
        assert!(u_ledger.finalize(&s1, 180));
        // Second slot with different nonce shares BaseWorkKey under AN1-TEST-USEFUL-ONCE-1 -> blocked!
        assert!(!u_ledger.finalize(&s2, 180));
        // Rollback restores all 4 consumption sets and credit
        u_ledger.rollback_last_final().unwrap();
        assert_eq!(u_ledger, before_final);
        assert!(u_ledger.finalize(&s2, 180));
    }
}
