//! **An IR class's weights are read within a budget the operator states** — ADR-0112's runtime
//! residency, for every IR class (`docs/design/palw/tir/runtime-residency.md`).
//!
//! Without a residency a PALWTIR1 artifact is mapped and every param bound in place
//! ([`super::artifact`]): every weight a kernel reads arrives through a page fault, which on the
//! fleet's virtio disks is 6–11 MB/s against 845 MB/s for a read sized to a tensor (ADR-0112 §1).
//! With one, **nothing is mapped**. The file is read through its descriptor, by positional reads
//! sized to what is needed, in three tiers ([`crate::tiers`], read off the program's dataflow):
//!
//! * **pinned** — every instance a forward reads whole: read once at open, in parallel, into memory
//!   this process owns and counts, and never given back;
//! * **routed** — the stacks a route selects rows of (a mixture's experts): an LRU of rows under
//!   what the budget leaves, a route's rows admitted together the moment its index is computed
//!   (ADR-0112 Decision 4) — every stack of the route group, missing rows read in parallel;
//! * **gathered** — the tables an input selects rows of (embeddings, per-layer embeddings, n-gram
//!   and positional tables): each row read when it is gathered, never resident whole.
//!
//! **The budget** is one number ([`TirResidencyPolicyV1`]): stated by the operator, or a fifth of
//! the weights within what the host can spare. **The floor** — the pinned set, one token's routed
//! rows and one admission in flight ([`TirResidencyArithmeticV1::floor_bytes`]) — is the least it
//! may be: a stated budget below it is refused by name with its terms; a default below it leaves the
//! class on the page cache and says why ([`TirResidencyDeclinedV1`]).
//!
//! **Opening is one pass.** Every byte of the file is read once, in inventory order, in chunks of
//! whole leaves (the next chunk read while the last is hashed): the inventory root — this node's
//! proof that it holds what the chain registered — through the consensus leaf and frontier, each
//! instance's `[min, max]` for the executor's refined plan, and the pinned set, kept. A court's tree
//! and its openings read again through the descriptor, a piece or a chunk at a time.
//!
//! **What it does not change** is one bit of what a kernel computes: a row is the mapping's
//! elements, the tiers decide where bytes are and never which, and the executor answers any read the
//! tiers did not plan with the whole instance, counted (`tests/residency_node.rs` holds the roots,
//! the leaves, the openings and the court's closes equal at the floor, at a fifth and through the
//! page cache).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_artifact::{PalwArtifactMerkleFrontierV1, artifact_leaf_parts_v1};
use kaspa_consensus_core::palw_tir_artifact_v1::{PALW_TIR_ROW_PIECE_BYTES_V1, palw_tir_inventory_leaf_count_v1};
use misaka_palw_tir::interval::Interval;
use misaka_palw_tir::{DType, TirProgramV1};
use misaka_palw_tir_artifact::PalwTirContainerV1;

use crate::elem::{Buf, Slice};
use crate::kernels::out_vec;
use crate::params::ParamData;
use crate::rows::{TirRowCountsV1, TirRowSourceV1, extend_le};
use crate::tiers::{TIR_RESIDENT_FRACTION_DENOMINATOR_V1, TirResidencyArithmeticV1, TirTierRulesV1, TirTierV1, TirTiersV1};

/// **How much of an IR class's weights this process keeps in memory** (ADR-0112 Decision 2) — the
/// same four answers as the Qwen3.6 runtime's policy, which the SDK maps onto this one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TirResidencyPolicyV1 {
    /// No residency: the file is mapped and the page cache decides — the path before this module,
    /// kept so the two can be measured against each other (`--palw-class-resident-bytes 0`).
    PageCache,
    /// Hold at most this many bytes of weights, the pinned set included — a STATED budget, refused
    /// below the class's floor.
    Bytes(u64),
    /// A fifth of the weights — the default where the host's spare memory cannot be read.
    FifthOfTheWeights,
    /// **The default where it can** (ADR-0112 Decision 2, amended 2026-09-11): a fifth of the
    /// weights if this many spare bytes hold it, the spare bytes if they hold less.
    FifthWithin(u64),
}

impl TirResidencyPolicyV1 {
    /// The budget in bytes for weights of `weight_bytes`, or `None` for the page cache.
    pub fn budget_for(self, weight_bytes: u64) -> Option<u64> {
        let fifth = weight_bytes.div_ceil(TIR_RESIDENT_FRACTION_DENOMINATOR_V1);
        match self {
            Self::PageCache => None,
            Self::Bytes(b) => Some(b),
            Self::FifthOfTheWeights => Some(fifth),
            Self::FifthWithin(spare) => Some(fifth.min(spare)),
        }
    }

    /// Did the operator state the number? A stated budget below the floor is refused; a default
    /// below it declines to the page cache — nobody asked for a number that cannot run.
    pub fn is_stated(self) -> bool {
        matches!(self, Self::Bytes(_))
    }
}

/// **Why a default budget was not taken** (ADR-0112 Decision 2, amended): what the default came
/// to, under the class's floor, the fifth it was measured from — and the weights and the routed
/// stacks, because a class that routes nothing has a floor near its size and is SUPPOSED to stay on
/// the page cache by default, where a mixture that does is the case ADR-0112 was written for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TirResidencyDeclinedV1 {
    pub budget_bytes: u64,
    pub floor_bytes: u64,
    pub fifth_bytes: u64,
    pub weight_bytes: u64,
    pub routed_bytes: u64,
}

/// **The residency's numbers**, for a log line or a test (ADR-0112 Decisions 7 and 8).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TirResidencyStatsV1 {
    pub budget_bytes: u64,
    pub pinned_bytes: u64,
    /// What the budget leaves the routed rows: the budget less the pinned set and one admission in
    /// flight.
    pub routed_capacity_bytes: u64,
    /// The routed rows held right now — never past the capacity between admissions.
    pub resident_routed_bytes: u64,
    /// One forward's routed rows: what the capacity is counted in tokens of.
    pub token_routed_bytes: u64,
    pub in_flight_bytes: u64,
    pub floor_bytes: u64,
    /// Routed rows found held (at an admission) and not (at an admission, or late at a gather).
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    /// Bytes read through the descriptor since open: the open pass (every byte once), then every
    /// routed miss, gathered row, opening piece and tree pass.
    pub bytes_read: u64,
    /// Of those, routed rows read on a miss.
    pub routed_bytes_read: u64,
    /// Gathered rows read, and their bytes.
    pub gathered_rows: u64,
    pub gathered_bytes: u64,
    /// Whole instances read for a dense read of a served one — zero on every path the tiers plan.
    pub whole_reads: u64,
}

impl TirResidencyStatsV1 {
    /// How many tokens of routed rows the capacity holds.
    pub fn tokens_held(&self) -> f64 {
        self.routed_capacity_bytes as f64 / self.token_routed_bytes.max(1) as f64
    }
}

/// **Bytes held 8-aligned**, so a typed view of a param's elements is in place.
pub struct TirHeldBytesV1 {
    words: Vec<u64>,
    len: usize,
}

impl TirHeldBytesV1 {
    pub fn zeroed(len: usize) -> Self {
        Self { words: vec![0u64; len.div_ceil(8)], len }
    }

    pub fn as_bytes(&self) -> &[u8] {
        // SAFETY: `words` holds at least `len` initialised bytes, and `u8` has no alignment.
        unsafe { std::slice::from_raw_parts(self.words.as_ptr() as *const u8, self.len) }
    }

    pub fn as_mut_bytes(&mut self) -> &mut [u8] {
        // SAFETY: as `as_bytes`; the borrow is unique.
        unsafe { std::slice::from_raw_parts_mut(self.words.as_mut_ptr() as *mut u8, self.len) }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// **A file read through its descriptor** — positional reads, never a mapping; every byte counted.
pub struct TirWeightFileV1 {
    #[cfg(unix)]
    file: std::fs::File,
    #[cfg(not(unix))]
    file: Mutex<std::fs::File>,
    path: PathBuf,
    len: u64,
    read: AtomicU64,
}

/// A read this large is split into parallel reads of this size (the device has a queue).
const PARALLEL_READ_BYTES: usize = 8 << 20;

impl TirWeightFileV1 {
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let file = std::fs::File::open(path)?;
        let len = file.metadata()?.len();
        #[cfg(not(unix))]
        let file = Mutex::new(file);
        Ok(Self { file, path: path.to_path_buf(), len, read: AtomicU64::new(0) })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Bytes read through this descriptor since it was opened.
    pub fn bytes_read(&self) -> u64 {
        self.read.load(Ordering::Relaxed)
    }

    /// `buf.len()` bytes from `offset`. A range that leaves the file is refused with the numbers —
    /// the offsets are a container's directory, which is data.
    pub fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), String> {
        if offset.checked_add(buf.len() as u64).is_none_or(|end| end > self.len) {
            return Err(format!("{}: {} bytes at {offset} leave a {}-byte file", self.path.display(), buf.len(), self.len));
        }
        #[cfg(unix)]
        let r = {
            use std::os::unix::fs::FileExt;
            self.file.read_exact_at(buf, offset)
        };
        #[cfg(not(unix))]
        let r = {
            use std::io::{Read, Seek, SeekFrom};
            let mut f = self.file.lock().expect("the file lock is never poisoned");
            f.seek(SeekFrom::Start(offset)).and_then(|_| f.read_exact(buf))
        };
        r.map_err(|e| format!("{}: unreadable at byte {offset}: {e}", self.path.display()))?;
        self.read.fetch_add(buf.len() as u64, Ordering::Relaxed);
        Ok(())
    }

    /// [`Self::read_at`], split into parallel reads for a large buffer.
    pub fn read_at_par(&self, offset: u64, buf: &mut [u8]) -> Result<(), String> {
        use rayon::prelude::*;
        if buf.len() <= PARALLEL_READ_BYTES {
            return self.read_at(offset, buf);
        }
        buf.par_chunks_mut(PARALLEL_READ_BYTES)
            .enumerate()
            .try_for_each(|(i, chunk)| self.read_at(offset + (i * PARALLEL_READ_BYTES) as u64, chunk))
    }
}

/// A pass reads whole leaves at a time, about this many bytes.
const PASS_CHUNK_BYTES: u64 = 32 << 20;

/// The chunks of an instance of `t` bytes in rows of `r`: whole rows when a row is under a chunk,
/// whole pieces of one row when it is not — so every chunk is a run of whole leaves.
fn chunks_of(t: u64, r: u64) -> Vec<(u64, u64)> {
    let piece = PALW_TIR_ROW_PIECE_BYTES_V1;
    let mut out = Vec::new();
    if t == 0 || r == 0 {
        return out;
    }
    if r <= PASS_CHUNK_BYTES {
        let per = (PASS_CHUNK_BYTES / r).max(1) * r;
        let mut s = 0;
        while s < t {
            let e = (s + per).min(t);
            out.push((s, e));
            s = e;
        }
    } else {
        let per = (PASS_CHUNK_BYTES / piece).max(1) * piece;
        let mut row = 0;
        while row < t {
            let mut a = 0;
            while a < r {
                let e = (a + per).min(r);
                out.push((row + a, row + e));
                a = e;
            }
            row += r;
        }
    }
    out
}

/// The leaves `(start, len)` of the chunk `[s, e)` of an instance in rows of `r` bytes.
fn leaves_of(s: u64, e: u64, r: u64) -> Vec<(u64, u64)> {
    let piece = PALW_TIR_ROW_PIECE_BYTES_V1;
    let mut out = Vec::new();
    let mut at = s;
    while at < e {
        let within = at % r;
        let len = (r - within).min(piece);
        out.push((at, len));
        at += len;
    }
    out
}

/// Bytes of one row of param `j` (an axis-0 slice at rank ≥ 2, the whole tensor below) — the
/// consensus inventory's row (`palw_tir_row_bytes_v1`).
fn row_bytes(p: &TirProgramV1, j: u16) -> u64 {
    let d = &p.params[j as usize];
    let w = d.dtype.width() as u64;
    if d.shape.len() >= 2 { d.shape[1..].iter().map(|x| *x as u64).product::<u64>() * w } else { instance_bytes(p, j) }
}

fn instance_bytes(p: &TirProgramV1, j: u16) -> u64 {
    let d = &p.params[j as usize];
    d.shape.iter().map(|x| *x as u64).product::<u64>() * d.dtype.width() as u64
}

/// A byte reader over instances: `(param, layer, at, buf)`.
pub type TirInstanceReaderV1<'r> = dyn Fn(u16, Option<u16>, u64, &mut [u8]) -> Result<(), String> + Sync + 'r;

/// **Every leaf hash of `instances`, in inventory order** — each instance read in chunks of whole
/// leaves through `read` (the next chunk read while the last is hashed, the leaves of a chunk hashed
/// in parallel), each leaf the consensus artifact leaf; `on_chunk` sees every chunk's bytes.
pub fn tir_stream_leaves_v1(
    program: &TirProgramV1,
    instances: &[(u16, Option<u16>)],
    read: &TirInstanceReaderV1<'_>,
    on_leaf: &mut dyn FnMut(Hash64),
    on_chunk: &mut dyn FnMut(u16, Option<u16>, &[u8]),
) -> Result<(), String> {
    use rayon::prelude::*;
    for &(j, layer) in instances {
        let name = &program.params[j as usize].name;
        let (t, r) = (instance_bytes(program, j), row_bytes(program, j));
        let chunks = chunks_of(t, r);
        let fetch = |(s, e): (u64, u64)| -> Result<TirHeldBytesV1, String> {
            let mut buf = TirHeldBytesV1::zeroed((e - s) as usize);
            read(j, layer, s, buf.as_mut_bytes())?;
            Ok(buf)
        };
        let Some(&first) = chunks.first() else { continue };
        let mut current = fetch(first)?;
        for (i, &(s, e)) in chunks.iter().enumerate() {
            let next = chunks.get(i + 1).copied();
            let hash = || -> Vec<Hash64> {
                leaves_of(s, e, r)
                    .par_iter()
                    .map(|&(at, len)| {
                        let off = (at - s) as usize;
                        artifact_leaf_parts_v1(name, layer, at as u32, &current.as_bytes()[off..off + len as usize])
                    })
                    .collect()
            };
            let (hashes, fetched) = match next {
                Some(n) => {
                    let (h, f) = rayon::join(hash, || fetch(n));
                    (h, Some(f?))
                }
                None => (hash(), None),
            };
            hashes.into_iter().for_each(&mut *on_leaf);
            on_chunk(j, layer, current.as_bytes());
            if let Some(f) = fetched {
                current = f;
            }
        }
    }
    Ok(())
}

/// `[min, max]` of little-endian `dtype` elements.
fn bytes_range(dtype: DType, bytes: &[u8]) -> Option<Interval> {
    fn typed<T: crate::elem::Elem>(bytes: &[u8]) -> Option<&[T]> {
        if !cfg!(target_endian = "little") {
            return None;
        }
        // SAFETY: every bit pattern is a value of the integer type T; `align_to` returns an aligned
        // middle part only.
        let (pre, mid, post) = unsafe { bytes.align_to::<T>() };
        (pre.is_empty() && post.is_empty()).then_some(mid)
    }
    let slice = match dtype {
        DType::I8 => typed::<i8>(bytes).map(Slice::I8),
        DType::I16 => typed::<i16>(bytes).map(Slice::I16),
        DType::I32 => typed::<i32>(bytes).map(Slice::I32),
        DType::I64 => typed::<i64>(bytes).map(Slice::I64),
        DType::Idx => typed::<u32>(bytes).map(Slice::Idx),
        DType::I128 => None,
    };
    match slice {
        Some(s) => crate::params::slice_range(s),
        None => crate::params::slice_range(Buf::from_le_bytes(dtype, bytes)?.slice()),
    }
}

/// One row of a routed instance.
type RowKey = (u16, Option<u16>, u32);

/// The routed rows held: an LRU, ordered by a use stamp.
#[derive(Default)]
struct TirRowCacheV1 {
    held: HashMap<RowKey, (Arc<TirHeldBytesV1>, u64)>,
    order: BTreeMap<u64, RowKey>,
    clock: u64,
    bytes: u64,
    hits: u64,
    misses: u64,
    evictions: u64,
    routed_bytes_read: u64,
}

impl TirRowCacheV1 {
    fn touch(&mut self, key: RowKey) -> Option<Arc<TirHeldBytesV1>> {
        let (row, stamp) = self.held.get_mut(&key)?;
        self.order.remove(stamp);
        self.clock += 1;
        *stamp = self.clock;
        self.order.insert(self.clock, key);
        Some(row.clone())
    }

    /// Hold `row` as the most recently used, and give back the coldest until the capacity holds. A
    /// row a gather holds stays alive through its handle; only the count drops — the in-flight term.
    fn insert(&mut self, key: RowKey, row: Arc<TirHeldBytesV1>, capacity: u64) {
        if self.touch(key).is_some() {
            return;
        }
        self.bytes += row.len() as u64;
        self.clock += 1;
        self.held.insert(key, (row, self.clock));
        self.order.insert(self.clock, key);
        while self.bytes > capacity {
            let Some((_, cold)) = self.order.pop_first() else { break };
            if let Some((gone, _)) = self.held.remove(&cold) {
                self.bytes = self.bytes.saturating_sub(gone.len() as u64);
                self.evictions += 1;
            }
        }
    }
}

/// **What opening a store came to.**
pub enum TirStoreOpenV1 {
    /// The page cache was asked for.
    PageCache,
    /// A default budget under the class's floor: the page cache, and why.
    Declined(TirResidencyDeclinedV1),
    Resident(Arc<TirWeightStoreV1>),
}

/// **The weights of one IR artifact, held within a budget** — one per artifact root in a process,
/// shared by every composite candidate of that parent ([`tir_weight_store_for_root_v1`]).
pub struct TirWeightStoreV1 {
    file: TirWeightFileV1,
    program: TirProgramV1,
    extents: BTreeMap<(u16, Option<u16>), (u64, u64)>,
    tiers: TirTiersV1,
    arithmetic: TirResidencyArithmeticV1,
    budget: u64,
    capacity: u64,
    pinned: BTreeMap<(u16, Option<u16>), TirHeldBytesV1>,
    ranges: BTreeMap<(u16, Option<u16>), Interval>,
    root: (Hash64, u32),
    cache: Mutex<TirRowCacheV1>,
    rows_gathered: AtomicU64,
    admissions: AtomicU64,
    gathered_rows: AtomicU64,
    gathered_bytes: AtomicU64,
    whole_reads: AtomicU64,
}

/// The stores this process holds, by artifact root — so every candidate of a parent shares one.
static TIR_WEIGHT_STORES_V1: Mutex<BTreeMap<Hash64, Weak<TirWeightStoreV1>>> = Mutex::new(BTreeMap::new());

/// **The live store of the artifact whose inventory root is `root`**, if this process holds one.
pub fn tir_weight_store_for_root_v1(root: &Hash64) -> Option<Arc<TirWeightStoreV1>> {
    let mut stores = TIR_WEIGHT_STORES_V1.lock().unwrap_or_else(|p| p.into_inner());
    stores.retain(|_, w| w.strong_count() > 0);
    stores.get(root).and_then(Weak::upgrade)
}

impl TirWeightStoreV1 {
    /// **Open `container`'s weights under `policy`** (see the module documentation): the tiers and
    /// their arithmetic, the budget against the floor, then the one pass that reads every byte — the
    /// pinned set kept, the root and the ranges computed.
    pub fn open(
        container: &PalwTirContainerV1,
        policy: TirResidencyPolicyV1,
        rules: TirTierRulesV1,
    ) -> Result<TirStoreOpenV1, String> {
        let program = &container.program;
        let tiers = TirTiersV1::of(program, rules);
        let a = tiers.arithmetic();
        let Some(budget) = policy.budget_for(a.weight_bytes) else { return Ok(TirStoreOpenV1::PageCache) };
        if budget < a.floor_bytes {
            if !policy.is_stated() {
                return Ok(TirStoreOpenV1::Declined(TirResidencyDeclinedV1 {
                    budget_bytes: budget,
                    floor_bytes: a.floor_bytes,
                    fifth_bytes: a.fifth_bytes,
                    weight_bytes: a.weight_bytes,
                    routed_bytes: a.routed_bytes,
                }));
            }
            return Err(format!(
                "a resident budget of {budget} bytes is below this class's floor of {}: the pinned set is {} bytes, one token's \
                 routed rows are {} and one admission in flight is {}; the smallest budget this class runs in is {} (ADR-0112 \
                 Decision 3, runtime-residency.md)",
                a.floor_bytes, a.pinned_bytes, a.routed_token_bytes, a.in_flight_bytes, a.floor_bytes
            ));
        }
        // The inventory's own refusals first (an instance past 4 GiB, a leaf count past u32): the pass
        // below hashes byte offsets as the consensus leaf does, and must never root what it refuses.
        let leaf_count = palw_tir_inventory_leaf_count_v1(program).map_err(|e| e.to_string())?;
        let file = TirWeightFileV1::open(&container.path).map_err(|e| format!("{}: {e}", container.path.display()))?;
        let instances: Vec<(u16, Option<u16>)> = crate::tiers::tir_param_instances_v1(program)
            .into_iter()
            .enumerate()
            .flat_map(|(j, inst)| inst.into_iter().map(move |l| (j as u16, l)))
            .collect();
        let mut extents = BTreeMap::new();
        for &(j, layer) in &instances {
            let at = container.locate(j, layer).ok_or_else(|| format!("the container has no tensor for param {j} at {layer:?}"))?;
            extents.insert((j, layer), at);
        }
        // The pinned set, read once — in parallel, because the reads are the whole cost of opening
        // under a budget and the device has a queue.
        use rayon::prelude::*;
        let pinned: BTreeMap<(u16, Option<u16>), TirHeldBytesV1> = instances
            .par_iter()
            .filter(|(j, _)| matches!(tiers.params[*j as usize].tier, TirTierV1::Pinned(_)))
            .map(|&(j, layer)| {
                let (off, len) = extents[&(j, layer)];
                let mut held = TirHeldBytesV1::zeroed(len as usize);
                file.read_at_par(off, held.as_mut_bytes())?;
                Ok(((j, layer), held))
            })
            .collect::<Result<_, String>>()?;
        // The one pass: every leaf, in inventory order — the pinned instances from memory, the rest
        // through the descriptor.
        let read = |j: u16, layer: Option<u16>, at: u64, buf: &mut [u8]| -> Result<(), String> {
            match pinned.get(&(j, layer)) {
                Some(held) => {
                    let at = at as usize;
                    buf.copy_from_slice(held.as_bytes().get(at..at + buf.len()).ok_or("a read past a pinned instance")?);
                    Ok(())
                }
                None => {
                    let (off, _) = extents[&(j, layer)];
                    file.read_at_par(off + at, buf)
                }
            }
        };
        let mut frontier = PalwArtifactMerkleFrontierV1::new();
        let mut ranges: BTreeMap<(u16, Option<u16>), Interval> = BTreeMap::new();
        tir_stream_leaves_v1(program, &instances, &read, &mut |leaf| frontier.push(leaf), &mut |j, layer, bytes| {
            if let Some(iv) = bytes_range(program.params[j as usize].dtype, bytes) {
                ranges.entry((j, layer)).and_modify(|r| *r = r.union(iv)).or_insert(iv);
            }
        })?;
        let count = u32::try_from(frontier.leaf_count()).map_err(|_| "more leaves than a u32 counts".to_string())?;
        if count != leaf_count {
            return Err(format!("the pass hashed {count} leaves of an inventory of {leaf_count}"));
        }
        let root = frontier.root().ok_or("the program's inventory has no leaf")?;
        let store = Arc::new(TirWeightStoreV1 {
            file,
            program: program.clone(),
            extents,
            arithmetic: a,
            capacity: a.routed_capacity(budget),
            budget,
            tiers,
            pinned,
            ranges,
            root: (root, count),
            cache: Mutex::new(TirRowCacheV1::default()),
            rows_gathered: AtomicU64::new(0),
            admissions: AtomicU64::new(0),
            gathered_rows: AtomicU64::new(0),
            gathered_bytes: AtomicU64::new(0),
            whole_reads: AtomicU64::new(0),
        });
        TIR_WEIGHT_STORES_V1.lock().unwrap_or_else(|p| p.into_inner()).insert(root, Arc::downgrade(&store));
        Ok(TirStoreOpenV1::Resident(store))
    }

    /// The program whose params this store holds (a composite's parent's).
    pub fn program(&self) -> &TirProgramV1 {
        &self.program
    }

    /// The inventory root and leaf count the open pass computed.
    pub fn root(&self) -> (Hash64, u32) {
        self.root
    }

    pub fn tiers(&self) -> &TirTiersV1 {
        &self.tiers
    }

    pub fn arithmetic(&self) -> TirResidencyArithmeticV1 {
        self.arithmetic
    }

    pub fn path(&self) -> &Path {
        self.file.path()
    }

    /// The pinned bytes of instance `(j, layer)`, if it is pinned.
    pub fn pinned(&self, j: u16, layer: Option<u16>) -> Option<&TirHeldBytesV1> {
        self.pinned.get(&(j, layer))
    }

    /// Is instance `(j, layer)` routed or gathered here?
    pub fn serves(&self, j: u16, layer: Option<u16>) -> bool {
        self.extents.contains_key(&(j, layer)) && self.tiers.params.get(j as usize).is_some_and(|t| t.tier.is_rows())
    }

    /// The precomputed `[min, max]` of instance `(j, layer)`.
    pub fn instance_range(&self, j: u16, layer: Option<u16>) -> Option<Interval> {
        self.ranges.get(&(j, layer)).copied()
    }

    /// **`buf.len()` bytes of instance `(j, layer)` from byte `at`** — from the pinned set, or
    /// through the descriptor: an opening's piece, a pass's chunk.
    pub fn read_bytes(&self, j: u16, layer: Option<u16>, at: u64, buf: &mut [u8]) -> Result<(), String> {
        if let Some(held) = self.pinned.get(&(j, layer)) {
            let at = usize::try_from(at).map_err(|_| "an offset past usize")?;
            let piece = held.as_bytes().get(at..at.saturating_add(buf.len())).ok_or("a read past a pinned instance")?;
            buf.copy_from_slice(piece);
            return Ok(());
        }
        let (off, len) = *self.extents.get(&(j, layer)).ok_or_else(|| format!("no tensor for param {j} at {layer:?}"))?;
        if at.checked_add(buf.len() as u64).is_none_or(|end| end > len) {
            return Err(format!("param {j} at {layer:?}: bytes {at}..+{} leave its {len}", buf.len()));
        }
        self.file.read_at_par(off + at, buf)
    }

    /// **The whole of instance `(j, layer)`**, held — a pinned one's bytes or a read of the file.
    pub fn read_instance(&self, j: u16, layer: Option<u16>) -> Result<TirHeldBytesV1, String> {
        let (_, len) = *self.extents.get(&(j, layer)).ok_or_else(|| format!("no tensor for param {j} at {layer:?}"))?;
        let mut held = TirHeldBytesV1::zeroed(len as usize);
        self.read_bytes(j, layer, 0, held.as_mut_bytes())?;
        Ok(held)
    }

    /// **The whole of a served instance's bytes** — the read the tiers never plan, counted
    /// ([`TirResidencyStatsV1::whole_reads`]).
    pub fn read_whole_bytes(&self, j: u16, layer: Option<u16>) -> Result<Vec<u8>, String> {
        let held = self.read_instance(j, layer)?;
        if !self.pinned.contains_key(&(j, layer)) {
            self.whole_reads.fetch_add(1, Ordering::Relaxed);
        }
        Ok(held.as_bytes().to_vec())
    }

    fn unit_bytes(&self, j: u16) -> u64 {
        let t = &self.tiers.params[j as usize];
        t.unit.saturating_mul(self.program.params[j as usize].dtype.width() as u64)
    }

    /// Rows `rows` of instance `(j, layer)` through the descriptor, in parallel, one read a row.
    fn read_rows(&self, j: u16, layer: Option<u16>, rows: &[u32]) -> Result<Vec<(u32, Arc<TirHeldBytesV1>)>, String> {
        use rayon::prelude::*;
        let (off, _) = *self.extents.get(&(j, layer)).ok_or_else(|| format!("no tensor for param {j} at {layer:?}"))?;
        let ub = self.unit_bytes(j);
        rows.par_iter()
            .map(|&r| {
                let mut held = TirHeldBytesV1::zeroed(ub as usize);
                self.file.read_at(off + r as u64 * ub, held.as_mut_bytes())?;
                Ok((r, Arc::new(held)))
            })
            .collect()
    }

    /// **The residency's numbers now.**
    pub fn stats(&self) -> TirResidencyStatsV1 {
        let cache = self.cache.lock().expect("the row cache is never poisoned");
        TirResidencyStatsV1 {
            budget_bytes: self.budget,
            pinned_bytes: self.arithmetic.pinned_bytes,
            routed_capacity_bytes: self.capacity,
            resident_routed_bytes: cache.bytes,
            token_routed_bytes: self.arithmetic.routed_token_bytes,
            in_flight_bytes: self.arithmetic.in_flight_bytes,
            floor_bytes: self.arithmetic.floor_bytes,
            hits: cache.hits,
            misses: cache.misses,
            evictions: cache.evictions,
            bytes_read: self.file.bytes_read(),
            routed_bytes_read: cache.routed_bytes_read,
            gathered_rows: self.gathered_rows.load(Ordering::Relaxed),
            gathered_bytes: self.gathered_bytes.load(Ordering::Relaxed),
            whole_reads: self.whole_reads.load(Ordering::Relaxed),
        }
    }

    /// **Every leaf hash of `instances`**, streamed through the store (a court's tree).
    pub fn leaf_hashes(&self, instances: &[(u16, Option<u16>)]) -> Result<Vec<Hash64>, String> {
        let mut leaves = Vec::new();
        tir_stream_leaves_v1(
            &self.program,
            instances,
            &|j, layer, at, buf| self.read_bytes(j, layer, at, buf),
            &mut |leaf| leaves.push(leaf),
            &mut |_, _, _| {},
        )?;
        Ok(leaves)
    }
}

impl TirRowSourceV1 for TirWeightStoreV1 {
    fn row_shape(&self, j: u16, layer: Option<u16>) -> Option<(u32, u64)> {
        self.serves(j, layer).then(|| (self.tiers.params[j as usize].rows, self.tiers.params[j as usize].unit))
    }

    /// The routed rows of the group not held, read together and in parallel, before any of them is
    /// computed. Gathered rows are read when they are gathered: they are not kept.
    fn admit(&self, wants: &[(u16, Option<u16>, &[u32])]) {
        self.admissions.fetch_add(1, Ordering::Relaxed);
        let mut missing: Vec<(u16, Option<u16>, Vec<u32>)> = Vec::new();
        {
            let mut cache = self.cache.lock().expect("the row cache is never poisoned");
            for &(j, layer, rows) in wants {
                if self.tiers.params.get(j as usize).map(|t| t.tier) != Some(TirTierV1::Routed) || !self.serves(j, layer) {
                    continue;
                }
                let mut mine = Vec::new();
                for r in rows.iter().copied().collect::<BTreeSet<u32>>() {
                    if cache.touch((j, layer, r)).is_some() {
                        cache.hits += 1;
                    } else {
                        cache.misses += 1;
                        mine.push(r);
                    }
                }
                if !mine.is_empty() {
                    missing.push((j, layer, mine));
                }
            }
        }
        if missing.is_empty() {
            return;
        }
        // Off the lock: the reads are the slow part. A read that fails here is not an error yet —
        // the gather that needs the row reads it again and reports the failure by name.
        use rayon::prelude::*;
        let read: Vec<(RowKey, Arc<TirHeldBytesV1>)> = missing
            .par_iter()
            .filter_map(|(j, layer, rows)| self.read_rows(*j, *layer, rows).ok().map(|v| (*j, *layer, v)))
            .flat_map_iter(|(j, layer, v)| v.into_iter().map(move |(r, row)| ((j, layer, r), row)))
            .collect();
        let mut cache = self.cache.lock().expect("the row cache is never poisoned");
        for (key, row) in read {
            cache.routed_bytes_read += row.len() as u64;
            cache.insert(key, row, self.capacity);
        }
    }

    fn gather_rows(&self, j: u16, layer: Option<u16>, rows: &[u32], out: &mut Buf) -> Result<(), String> {
        if !self.serves(j, layer) {
            return Err(format!("param {j} (layer {layer:?}) is not served by rows"));
        }
        let dtype = self.program.params[j as usize].dtype;
        let routed = self.tiers.params[j as usize].tier == TirTierV1::Routed;
        let mut found: BTreeMap<u32, Arc<TirHeldBytesV1>> = BTreeMap::new();
        if routed {
            let mut cache = self.cache.lock().expect("the row cache is never poisoned");
            for &r in rows {
                if let Some(row) = cache.touch((j, layer, r)) {
                    found.insert(r, row);
                }
            }
        }
        let missing: Vec<u32> = rows.iter().copied().filter(|r| !found.contains_key(r)).collect::<BTreeSet<_>>().into_iter().collect();
        if !missing.is_empty() {
            let read = self.read_rows(j, layer, &missing)?;
            if routed {
                // Late: a row this gather needs was not admitted, or was evicted since (another
                // duty's forward on the same store). Read now, and held.
                let mut cache = self.cache.lock().expect("the row cache is never poisoned");
                for (r, row) in &read {
                    cache.misses += 1;
                    cache.routed_bytes_read += row.len() as u64;
                    cache.insert((j, layer, *r), row.clone(), self.capacity);
                }
            } else {
                self.gathered_rows.fetch_add(read.len() as u64, Ordering::Relaxed);
                self.gathered_bytes.fetch_add(read.iter().map(|(_, b)| b.len() as u64).sum(), Ordering::Relaxed);
            }
            found.extend(read);
        }
        crate::with_dtype!(dtype, T => {
            let o = out_vec::<T>(out);
            o.clear();
            o.reserve(rows.len() * self.tiers.params[j as usize].unit as usize);
            for r in rows {
                extend_le(o, found[r].as_bytes());
            }
        });
        self.rows_gathered.fetch_add(rows.len() as u64, Ordering::Relaxed);
        Ok(())
    }

    fn read_whole(&self, j: u16, layer: Option<u16>) -> Result<ParamData<'static>, String> {
        let held = self.read_instance(j, layer)?;
        self.whole_reads.fetch_add(1, Ordering::Relaxed);
        let buf = Buf::from_le_bytes(self.program.params[j as usize].dtype, held.as_bytes()).ok_or("whole elements")?;
        ParamData::from_buf(buf).map_err(|e| e.to_string())
    }

    fn range(&self, j: u16, layer: Option<u16>) -> Option<Interval> {
        self.instance_range(j, layer)
    }

    fn counts(&self) -> TirRowCountsV1 {
        TirRowCountsV1 {
            rows_gathered: self.rows_gathered.load(Ordering::Relaxed),
            admissions: self.admissions.load(Ordering::Relaxed),
            whole_reads: self.whole_reads.load(Ordering::Relaxed),
        }
    }
}
