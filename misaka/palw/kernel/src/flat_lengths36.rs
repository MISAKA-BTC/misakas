//! Versioned job-length contract, version 36 (RFC02 §II.14.59). A length change is a
//! NEW contract version, never a reinterpretation of an older field: v1 programs keep
//! their frozen `history_bound`, the v8 window/selection relation carries its own plan,
//! and RFC03 patch/audio axes exist only as fenced, refused declarations until their
//! own typed stage contract and fence ship. Unverified source checkpoint (2026-10-15).
use crate::{
    flat_history8::{HISTORY_SELECTION_MAX_POSITIONS_V8, HistorySelectionPlanV8, HistorySelectionScopeV8},
    flat_lengths7::{FlatJobLengthsV7, FlatLengthAxisV7},
    hash::{Digest, object_id},
};
use borsh::{BorshDeserialize, BorshSerialize};

pub const LENGTH_CONTRACT_VERSION_V36: u16 = 36;
pub const LENGTH_CONTRACT_MAX_BYTES_V36: usize = 4096;
/// RFC03 patch/audio lengths are specified but NOT armable in this release: a contract
/// declaring them decodes (so bytes ride unjudged below their fence) and is refused by name.
pub const MEDIA_LENGTHS_ARMABLE_V36: bool = false;

/// How the history axis grows with the token length.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum HistoryRuleV36 {
    /// v1 semantics, unchanged: every position < the program's frozen `history_bound`.
    FrozenV1 { history_bound: u32 },
    /// v8 bounded window with score-descending selection (`flat_history8`). The plan root
    /// binds graph coordinates, row shape, window and selection cap; long-context cost
    /// and evidence bounds derive from that plan at the requested maximum.
    Window8 { plan: Digest, window: u32, selection_cap: u32 },
}
/// One declared modality axis. `unit_bytes` is the exact committed input size of one
/// unit (token id = 4; one patch = channels·h·w·dtype width; one audio frame = its
/// samples·width). `profile` names the RFC03 encoding/preprocessing profile (zero for tokens).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ModalityLengthV36 {
    pub axis: FlatLengthAxisV7,
    pub maximum: u32,
    pub unit_bytes: u32,
    pub profile: Digest,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct LengthContractV36 {
    pub version: u16,
    pub program_root: Digest,
    pub rule: HistoryRuleV36,
    pub maximum_positions: u32,
    pub token: ModalityLengthV36,
    pub patch: Option<ModalityLengthV36>,
    pub audio: Option<ModalityLengthV36>,
}
/// Checked contract plus, for `Window8`, its independently derived scope.
pub struct CheckedLengthContractV36 {
    contract: LengthContractV36,
    history_scope: Option<HistorySelectionScopeV8>,
}
impl CheckedLengthContractV36 {
    pub fn contract(&self) -> &LengthContractV36 {
        &self.contract
    }
    pub fn history_scope(&self) -> Option<&HistorySelectionScopeV8> {
        self.history_scope.as_ref()
    }
    pub fn maxima(&self) -> FlatJobLengthsV7 {
        FlatJobLengthsV7 {
            token: self.contract.token.maximum,
            patch: self.contract.patch.map_or(0, |m| m.maximum),
            audio: self.contract.audio.map_or(0, |m| m.maximum),
        }
    }
    /// A job's committed actual lengths: every present axis 1..=maximum, every absent axis 0.
    /// No claim or later slice may enlarge them (the caller binds this root before Fund).
    pub fn check_actual(&self, actual: FlatJobLengthsV7, positions: u32) -> Result<Digest, String> {
        actual.within(self.maxima())?;
        if positions == 0 || positions > self.contract.maximum_positions || actual.token > positions {
            return Err("length36 actual positions exceed the versioned maximum".into());
        }
        Ok(object_id(b"misaka-palw/kernel/job-actual-lengths/v36", &(self.contract.root(), actual, positions)))
    }
}
impl LengthContractV36 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/versioned-length-contract/v36", self)
    }
    /// Bounded canonical decode (fixed-size body, two optional fixed-size axes).
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        if raw.len() > LENGTH_CONTRACT_MAX_BYTES_V36 {
            return Err("length36 contract byte cap".into());
        }
        let c: Self = borsh::from_slice(raw).map_err(|_| "length36 contract grammar")?;
        if borsh::to_vec(&c).map_err(|e| e.to_string())? != raw {
            return Err("length36 noncanonical contract".into());
        }
        Ok(c)
    }
    /// `program_history_bound` is the v1 program's frozen header field; `plan` must be
    /// supplied for `Window8` and must hash to the declared root.
    pub fn check(
        &self,
        program_history_bound: u32,
        plan: Option<&HistorySelectionPlanV8>,
    ) -> Result<CheckedLengthContractV36, String> {
        if self.version != LENGTH_CONTRACT_VERSION_V36 || self.program_root == [0; 64] || self.maximum_positions == 0 {
            return Err("length36 version/program/maximum".into());
        }
        if self.token.axis != FlatLengthAxisV7::Token
            || self.token.unit_bytes != 4
            || self.token.profile != [0; 64]
            || self.token.maximum == 0
            || self.token.maximum > self.maximum_positions
        {
            return Err("length36 token axis must be the exact u32-id axis within the positions maximum".into());
        }
        for (m, axis) in [(self.patch, FlatLengthAxisV7::Patch), (self.audio, FlatLengthAxisV7::Audio)] {
            if let Some(m) = m {
                if m.axis != axis || m.maximum == 0 || m.unit_bytes == 0 || m.profile == [0; 64] {
                    return Err("length36 media axis declaration incomplete".into());
                }
                if !MEDIA_LENGTHS_ARMABLE_V36 {
                    return Err("length36 RFC03 patch/audio lengths require their own typed stage contract and fence".into());
                }
            }
        }
        let history_scope = match self.rule {
            HistoryRuleV36::FrozenV1 { history_bound } => {
                if history_bound != program_history_bound || self.maximum_positions > history_bound || plan.is_some() {
                    return Err("length36 v1 rule must keep the program's frozen history bound".into());
                }
                None
            }
            HistoryRuleV36::Window8 { plan: root, window, selection_cap } => {
                let p = plan.ok_or("length36 Window8 requires its history-selection plan")?;
                p.validate()?;
                if p.root() != root
                    || p.window != window
                    || p.selection_cap != selection_cap
                    || p.max_positions != self.maximum_positions
                    || self.maximum_positions > HISTORY_SELECTION_MAX_POSITIONS_V8
                {
                    return Err("length36 Window8 plan/window/cap/maximum differ".into());
                }
                Some(p.scope()?)
            }
        };
        Ok(CheckedLengthContractV36 { contract: self.clone(), history_scope })
    }
}
