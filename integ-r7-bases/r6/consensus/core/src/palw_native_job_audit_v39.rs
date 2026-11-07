//! Checked readonly job projection. Its only factory is
//! `PalwChainStateV2::native_job_audit_from_stream_v39` (the job fold's audit
//! module), which re-derives every field from one fully authenticated original
//! aux stream under the auditor's own Params; public wire values cannot
//! construct this capability. Data identity, escrow settlement and mining
//! approval are separate verification questions.
use crate::{Hash64, palw_native_job_v39::*};
use misaka_palw_kernel::{flat_job39::CheckedJobMaximumV39, hash::Digest};

pub struct CheckedNativeJobAuditV39 {
    pub(crate) book: NativeJobBookV39,
    pub(crate) maximum: CheckedJobMaximumV39,
    pub(crate) credit: Option<NativeSingleCreditV39>,
    pub(crate) state: Hash64,
    pub(crate) auxiliary: Hash64,
    pub(crate) daa: u64,
    pub(crate) master: Digest,
    pub(crate) stage: Option<u16>,
}
impl CheckedNativeJobAuditV39 {
    pub fn original(&self) -> &NativeJobOriginalV39 {
        self.book.original()
    }
    pub fn maximum(&self) -> &CheckedJobMaximumV39 {
        &self.maximum
    }
    pub fn book(&self) -> &NativeJobBookV39 {
        &self.book
    }
    pub fn claim(&self) -> Option<&NativeAcceptedClaimV39> {
        self.book.claim()
    }
    pub fn terminal(&self) -> NativeJobTerminalV39 {
        self.book.terminal()
    }
    pub fn settlement(&self) -> Option<&NativeSettlementV39> {
        self.book.settlement()
    }
    pub fn credit(&self) -> Option<&NativeSingleCreditV39> {
        self.credit.as_ref()
    }
    pub fn single_credit(&self) -> Option<&NativeSingleCreditV39> {
        self.credit()
    }
    pub fn state_root(&self) -> Hash64 {
        self.state
    }
    pub fn auxiliary_root(&self) -> Hash64 {
        self.auxiliary
    }
    pub fn daa(&self) -> u64 {
        self.daa
    }
    pub fn master(&self) -> Digest {
        self.master
    }
    pub fn stage(&self) -> Option<u16> {
        self.stage
    }
}
