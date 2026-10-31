//! Read-only connection from an actual native consensus session to the Miner
//! responder. DTOs and arbitrary implementations are not native authority.
//! The production implementation has a private constructor in consensus and
//! rechecks its selected state/accepted prefix before EVERY authorize/status.
//! Kaspad obtains it from its actual ConsensusProxy, never from peer DTOs.
use crate::{BlockHash, Hash64};
use misaka_palw_positive_contract::messages::{AggregateBudget, MinerDataStatus, MinerResultCommit, SourceRequest};

/// Native-resolved snapshot identity; not a peer's branch label or a credit
/// token. Generation comes from the actual native/import owner, not a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PositiveSourceViewIdV2 {
    pub selected_parent: BlockHash,
    pub state_root: Hash64,
    pub accepted_prefix: Hash64,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PositiveSourceReadBoundsV2 {
    pub source_units: u64,
    pub max_unit_bytes: u64,
    pub work_per_read: u64,
    pub budget: AggregateBudget,
    pub now: u64,
    /// Exclusive service-duty cutoff, independently of funding admission expiry.
    pub cutoff: u64,
}

/// Per-unit G14 geometry. Raw unique source data and transmitted authenticated
/// opening bytes are different budget dimensions, including at a short tail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PositiveSourceUnitBoundsV2 {
    pub data_bytes: u64,
    /// Upper bound for this existing raw G14 response body. Native outer
    /// framing/signatures/carriers are charged separately by the native quote.
    pub body_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PositiveSourceReadErrorV2 {
    Unavailable,
    Inactive,
    Missing,
    Binding,
    StaleView,
    Range,
    Deadline,
    Budget,
    UnsupportedSource,
}

/// Interface, not a successful default implementation. Only the actual native
/// session may resolve accepted op3 requests, current literal unit receipts,
/// source geometry, funding/role/reservation facts and exclusive cutoffs.
/// A local preparation journal and Panel ACK cannot supply Complete here.
/// No method creates a payment, default, arithmetic verdict or credit grant.
pub trait PositiveSourceReadV2: Send + Sync {
    fn request(&self) -> &SourceRequest;
    fn view_id(&self) -> PositiveSourceViewIdV2;
    fn authorize(&self, result: &MinerResultCommit) -> Result<PositiveSourceReadBoundsV2, PositiveSourceReadErrorV2>;
    fn unit_bounds(&self, unit: u64) -> Result<PositiveSourceUnitBoundsV2, PositiveSourceReadErrorV2>;
    fn status(&self, unit: u64) -> Result<MinerDataStatus, PositiveSourceReadErrorV2>;
}
