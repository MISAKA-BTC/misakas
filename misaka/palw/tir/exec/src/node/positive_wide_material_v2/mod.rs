//! Explicit wide Panel production from ONE original CPU run. Local data only:
//! no native permission, protocol identity, new wire/domain or saved verdict.
//! Node still authenticates canonical sources and independently evaluates them.
//! Tape offsets/collector values are never authenticated Node operands.
mod capture;
pub mod claims_tape;
mod proof;
mod trace;
use super::{
    memory::{TirMemoryLeaseV1, TirMemoryReserverV1},
    run_wide_q24_v2::TirJobRunWideQ24V2,
};
pub use capture::{CaptureExpectedV2, WidePanelCaptureV2, capture_one_pass_v2};
use kaspa_consensus_core::{
    Hash64,
    palw_tir_class_v1::PalwTirClassV1,
    palw_tir_step_v1::{PalwTirStepBindingV1, PalwTirStepSpaceV2},
    palw_v2::PalwJobContextV2,
};
use misaka_palw_tir::{DType, types::MAX_RANK};
pub use proof::{WidePanelBuilderV2, WidePanelMaterialV2, WidePanelOwnersV2, WidePanelTargetV2, WideProviderEnvelopeV2};
use std::{
    cell::RefCell,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    mem::size_of,
};
pub type Error = String;
fn add(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_add(b).ok_or_else(|| "wide Panel addition overflow".into())
}
fn mul(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_mul(b).ok_or_else(|| "wide Panel multiplication overflow".into())
}
fn count(n: usize) -> Result<u64, Error> {
    u64::try_from(n).map_err(|_| "wide Panel index width".into())
}
fn vec_exact<T>(n: usize) -> Result<Vec<T>, Error> {
    let mut v = Vec::new();
    v.try_reserve_exact(n).map_err(|_| "wide Panel allocation")?;
    if v.capacity() != n {
        return Err("wide Panel excess capacity".into());
    }
    Ok(v)
}
#[derive(Clone, Copy, Debug)]
pub struct WidePanelLimitsV2 {
    pub max_records: u64,
    pub max_step_leaves: u64,
    pub max_tape_bytes: u64,
    pub max_live_heap_bytes: u64,
    pub max_visits: u64,
    pub max_io_bytes: u64,
    pub max_hash_work: u64,
    pub max_proof_attempts: u64,
    pub max_provider_bytes: u64,
    pub max_provider_scratch: u64,
    pub max_material_bytes: u64,
    pub max_claim_bytes: u64,
    pub max_trace_frames: u64,
    /// Existing plan/model/input/frame/signature/Node owners, pre-summed by caller.
    /// This does not reserve or authenticate those owners on their behalf.
    pub external_retained_bytes: u64,
}
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct WidePanelMeterV2 {
    pub visits: u64,
    pub io_bytes: u64,
    pub hash_work: u64,
    pub proof_attempts: u64,
    pub provider_bytes: u64,
    pub copied_bytes: u64,
    pub tape_bytes: u64,
}
/// Separate operational disk quota owner. It must acquire the entire quoted
/// simultaneous tape capacity before returning its guard and release on Drop.
/// This is NOT the RAM reserver and grants no native storage/credit authority.
pub type WidePanelDiskReserverV2 = std::sync::Arc<dyn Fn(Hash64, Hash64, u64) -> Result<TirMemoryLeaseV1, Error> + Send + Sync>;
/// One local Panel attempt's immutable limits and spent work. Clones share
/// counters; a failed capture does not erase them or permit another CPU run.
/// The native/service owner must retain this handle for its attempt. Creating
/// a new local handle is not native permission, funding or a budget reset grant.
#[derive(Clone)]
pub struct WidePanelWorkV2 {
    inner: std::rc::Rc<Session>,
    class: Hash64,
    execution: Hash64,
    started: std::rc::Rc<std::cell::Cell<bool>>,
}
impl WidePanelWorkV2 {
    pub fn new(limits: WidePanelLimitsV2, class: Hash64, execution: Hash64, disk: &WidePanelDiskReserverV2) -> Result<Self, Error> {
        if [
            limits.max_records,
            limits.max_step_leaves,
            limits.max_tape_bytes,
            limits.max_live_heap_bytes,
            limits.max_visits,
            limits.max_io_bytes,
            limits.max_hash_work,
            limits.max_proof_attempts,
            limits.max_provider_bytes,
            limits.max_provider_scratch,
            limits.max_material_bytes,
            limits.max_claim_bytes,
            limits.max_trace_frames,
        ]
        .contains(&0)
        {
            return Err("wide Panel finite nonzero ceilings required".into());
        }
        Ok(Self {
            inner: std::rc::Rc::new(Session::new(limits, disk(class, execution, limits.max_tape_bytes)?)?),
            class,
            execution,
            started: std::rc::Rc::new(std::cell::Cell::new(false)),
        })
    }
    pub fn meter(&self) -> WidePanelMeterV2 {
        *self.inner.meter.borrow()
    }
    /// Same-crate Loaded bridge only. Preserve this shared one-use/cumulative
    /// ledger; do not reconstruct a Work from a peer scalar or replace it for
    /// runner creation. This check/hash-free charge precedes its one borrowed
    /// program comparison, without exposing the private plan to the caller.
    #[cfg(feature = "positive-panel")]
    pub(crate) fn admit_loaded_runner_v2(
        &self,
        external_floor: u64,
        class: Hash64,
        execution: Hash64,
    ) -> Result<(), Error> {
        if self.started.get() {
            return Err("wide attempt CPU run already started; spent work is retained".into());
        }
        if self.class != class || self.execution != execution {
            return Err("wide Loaded work original scope mismatch".into());
        }
        if self.inner.limits.external_retained_bytes < external_floor {
            return Err("wide Loaded external owner floor".into());
        }
        // The SAME existing capture metadata-visit currency, one additional
        // traversal for runner.new's structural equality. It is not RAM bytes
        // or CPU instructions. Refusals never refund earlier shared charges.
        self.inner.charge(
            Charge::Visit,
            mul(add(misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64, 1024)?, 128)?,
        )
    }
}
struct Session {
    _disk_lease: TirMemoryLeaseV1,
    limits: WidePanelLimitsV2,
    meter: RefCell<WidePanelMeterV2>,
}
#[derive(Clone, Copy)]
enum Charge {
    Visit,
    Io,
    Hash,
    Proof,
    Provider,
    Copy,
    Tape,
}
impl Session {
    fn new(l: WidePanelLimitsV2, disk_lease: TirMemoryLeaseV1) -> Result<Self, Error> {
        if [
            l.max_records,
            l.max_step_leaves,
            l.max_tape_bytes,
            l.max_live_heap_bytes,
            l.max_visits,
            l.max_io_bytes,
            l.max_hash_work,
            l.max_proof_attempts,
            l.max_provider_bytes,
            l.max_provider_scratch,
            l.max_material_bytes,
            l.max_claim_bytes,
            l.max_trace_frames,
        ]
        .contains(&0)
        {
            return Err("wide Panel finite nonzero ceilings required".into());
        }
        Ok(Self { _disk_lease: disk_lease, limits: l, meter: RefCell::new(WidePanelMeterV2::default()) })
    }
    fn charge(&self, k: Charge, n: u64) -> Result<(), Error> {
        let mut m = self.meter.borrow_mut();
        let (v, cap) = match k {
            Charge::Visit => (&mut m.visits, self.limits.max_visits),
            Charge::Io => (&mut m.io_bytes, self.limits.max_io_bytes),
            Charge::Hash => (&mut m.hash_work, self.limits.max_hash_work),
            Charge::Proof => (&mut m.proof_attempts, self.limits.max_proof_attempts),
            Charge::Provider => (&mut m.provider_bytes, self.limits.max_provider_bytes),
            Charge::Copy => (&mut m.copied_bytes, self.limits.max_io_bytes),
            Charge::Tape => (&mut m.tape_bytes, self.limits.max_tape_bytes),
        };
        let next = add(*v, n)?;
        if next > cap {
            return Err("wide Panel cumulative budget".into());
        }
        *v = next;
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Node(u32, u16, u16),
    Fixed(u32, u16, Option<u16>),
    History(u32, u16, Option<u16>),
}
#[derive(Clone, Copy)]
enum Storage {
    Bytes(u64),
    StepView { position: u32, occurrence: u16, node: u16 },
    HistoryView { state: u16, layer: Option<u16>, first: u32, row_lanes: u64 },
}
struct Record {
    key: Key,
    dtype: DType,
    rank: usize,
    shape: [usize; MAX_RANK],
    lanes: u64,
    storage: Storage,
}
impl Record {
    fn shape(&self) -> &[usize] {
        &self.shape[..self.rank]
    }
}
struct Tape {
    file: RefCell<File>,
    len: u64,
}
impl Tape {
    fn empty(file: File) -> Result<Self, Error> {
        if file.metadata().map_err(|e| e.to_string())?.len() != 0 {
            return Err("wide Panel tape must be empty and exclusively acquired".into());
        }
        Ok(Self { file: RefCell::new(file), len: 0 })
    }
    fn append(&mut self, b: &[u8], session: &Session) -> Result<u64, Error> {
        let end = add(self.len, count(b.len())?)?;
        if end > session.limits.max_tape_bytes {
            return Err("wide Panel tape cap".into());
        }
        session.charge(Charge::Tape, count(b.len())?)?;
        session.charge(Charge::Io, count(b.len())?)?;
        let at = self.len;
        self.file.get_mut().seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
        self.file.get_mut().write_all(b).map_err(|e| e.to_string())?;
        self.len = end;
        Ok(at)
    }
    fn read(&self, at: u64, b: &mut [u8], session: &Session) -> Result<(), Error> {
        if add(at, count(b.len())?)? > self.len {
            return Err("wide Panel tape range".into());
        }
        session.charge(Charge::Io, count(b.len())?)?;
        let mut f = self.file.borrow_mut();
        f.seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
        f.read_exact(b).map_err(|e| e.to_string())
    }
}
struct Shape([usize; MAX_RANK], usize);
impl Shape {
    fn new(s: &[usize]) -> Result<Self, Error> {
        if s.len() > MAX_RANK {
            return Err("wide Panel rank".into());
        }
        let mut a = [0; MAX_RANK];
        a[..s.len()].copy_from_slice(s);
        Ok(Self(a, s.len()))
    }
}
fn decode(dtype: DType, b: &[u8]) -> i128 {
    dtype.decode_le(b)
}
