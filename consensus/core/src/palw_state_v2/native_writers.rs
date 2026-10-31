//! RFC-0006: before-allocation admission for the remaining shared V2 writers — payouts, exposure, court groups,
//! kernel-route ledger/aux rows, derived indexes and note/counter journals.
//!
//! Every quote here is a typed request computed from the ACTUAL value the writer is about to insert (its deep-copy
//! heap, the absent BTree membership it creates, the derived index nodes it adds, the canonical Borsh buffers it
//! encodes) plus the journal slots it pushes. The quote is admitted into the transition's construction owner
//! (`NativeTransitionReservationV1`) BEFORE the writer allocates; a refusal returns
//! `PalwStateV2Error::NativeResource`, the typed retryable LOCAL capacity error — never a block, object or peer
//! invalidity. With native capacity tracking off (every preset today) each admission returns `Ok(())` before any
//! work, so the historical writers, their allocation strategy and every byte they journal are unchanged.
//!
//! What this does NOT cover (kept explicit, never implied): caller-side buffers a legacy caller encodes BEFORE it
//! calls a byte-row writer (their retained lifetime is admitted here, their allocation instant precedes it — use
//! [`TransitionBuilder::write_kernel_row_encoded_v1`] to admit before encoding), allocator arenas/RSS, and the
//! physical hard quota. Those remain separate coverage.
use super::*;
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeHeapV1, TirNativeRequestsV1, typed::BTreeNodeEnvelopeV1};

pub(super) fn native_err_v1(coverage: NativeCoverageV1) -> PalwStateV2Error {
    PalwStateV2Error::NativeResource(coverage)
}

/// **A lenient consumer may drop a SEMANTIC refusal, never a local capacity refusal.** Several folds (the closing kernel tick,
/// the mesh sweep, chunk-group forfeits, deadline re-arms) deliberately ignore a writer's refusal (`let _ =`, `Err(_) => {}`,
/// `.is_err()`). Once those writers admit native capacity, ignoring their `NativeResource` would make a capacity-starved node
/// compute a DIFFERENT state (a skipped payout, a missing deadline) instead of refusing the transition locally and retrying —
/// a consensus split. Every such site routes the result through here: a capacity refusal propagates (`?` on the outer
/// `Result`), any other outcome is handed back unchanged for the site's historical leniency. Flags-off nothing can return
/// `NativeResource` from these writers, so the historical behavior is byte-identical.
pub(super) fn local_capacity_or_v1<T>(result: Result<T, PalwStateV2Error>) -> Result<Result<T, PalwStateV2Error>, PalwStateV2Error> {
    match result {
        Err(local @ PalwStateV2Error::NativeResource(_)) => Err(local),
        other => Ok(other),
    }
}

/// A typed request builder for one writer (or one fold's scratch), with the journal slots it pushes.
pub(super) struct NativeWriteQuoteV1 {
    q: TirNativeRequestsV1,
    slots: usize,
    path: &'static str,
}

impl NativeWriteQuoteV1 {
    pub(super) fn new(path: &'static str) -> Self {
        Self { q: TirNativeRequestsV1::new(), slots: 0, path }
    }
    /// One deep copy of an actual typed value (a map clone, a journal clone, a scratch copy).
    pub(super) fn copy<T: NativeHeapV1 + ?Sized>(&mut self, value: &T) -> Result<&mut Self, PalwStateV2Error> {
        let heap = value.native_heap_v1(self.path);
        self.q.add_v1(heap.bytes().ok_or_else(|| native_err_v1(heap))?).map_err(native_err_v1)?;
        Ok(self)
    }
    /// `n` new memberships of a `BTreeMap<K, V>` (a `BTreeSet<T>` is `BTreeMap<T, ()>`), one pinned node envelope each.
    pub(super) fn member<K, V>(&mut self, n: u64) -> Result<&mut Self, PalwStateV2Error> {
        if n == 0 {
            return Ok(self);
        }
        if cfg!(not(target_pointer_width = "64")) {
            return Err(native_err_v1(NativeCoverageV1::unsupported::<BTreeMap<K, V>>(
                self.path,
                "pinned BTree allocation layout only covers 64-bit targets",
            )));
        }
        self.q.exact_v1::<BTreeNodeEnvelopeV1<K, V>>(n).map_err(native_err_v1)?;
        Ok(self)
    }
    /// A membership only when the key is actually absent and the write inserts.
    pub(super) fn member_if<K, V>(&mut self, absent: bool) -> Result<&mut Self, PalwStateV2Error> {
        self.member::<K, V>(u64::from(absent))
    }
    /// Exactly `n` elements of `T` (an exact-capacity clone, e.g. `Vec<u8>::clone`).
    pub(super) fn exact<T>(&mut self, n: u64) -> Result<&mut Self, PalwStateV2Error> {
        self.q.exact_v1::<T>(n).map_err(native_err_v1)?;
        Ok(self)
    }
    /// A growing `Vec<T>` that reaches `n` elements (pinned minimum capacity and geometric growth).
    pub(super) fn grown<T>(&mut self, n: u64) -> Result<&mut Self, PalwStateV2Error> {
        self.q.vec_v1::<T>(n).map_err(native_err_v1)?;
        Ok(self)
    }
    /// `borsh::to_vec(value)`: pinned Borsh starts at its 1,024-byte serializer capacity and grows geometrically;
    /// the canonical length is computed without allocating (`borsh::object_length`).
    pub(super) fn encoded<T: borsh::BorshSerialize + ?Sized>(&mut self, value: &T) -> Result<&mut Self, PalwStateV2Error> {
        let n = borsh::object_length(value)
            .map_err(|e| native_err_v1(NativeCoverageV1::unsupported::<T>(self.path, &format!("canonical length: {e}"))))?
            as u64;
        self.q.vec_v1::<u8>(n.max(1024)).map_err(native_err_v1)?;
        Ok(self)
    }
    pub(super) fn slots(&mut self, n: usize) -> Result<&mut Self, PalwStateV2Error> {
        self.slots = self
            .slots
            .checked_add(n)
            .ok_or_else(|| native_err_v1(NativeCoverageV1::overflow::<PalwDeltaEntryV2>("native writer journal slots")))?;
        Ok(self)
    }
}

impl<'a> TransitionBuilder<'a> {
    /// Whether a writer must quote at all. Off on every preset: the writer then returns `Ok(())` before computing anything.
    pub(super) fn native_writers_tracking_v1(&self) -> bool {
        self.native_reservation.is_tracking_v1()
    }

    /// Admit one writer's quote (typed requests and journal slots) before its first allocation.
    pub(super) fn admit_native_write_v1(&mut self, quote: NativeWriteQuoteV1) -> Result<(), PalwStateV2Error> {
        if !self.native_writers_tracking_v1() {
            return Ok(());
        }
        self.reserve_mutation_sequence_v1(quote.q.bytes_v1(), quote.slots)
    }

    /// **The standard keyed-row writer** over the actual state map `map`: the incoming row's map clone and its journal
    /// copy (two deep copies of `new`), two key copies, the membership when the key is actually absent and the write
    /// inserts, the old-row deep clone when the writer reads `get(..).cloned()` (`clone_old`), and one journal slot.
    /// Removal moves the old row into the journal and needs no copy.
    pub(super) fn reserve_map_row_v1<K, V>(
        &mut self,
        key: &K,
        new: &Option<V>,
        map: fn(&PalwChainStateV2) -> &BTreeMap<K, V>,
        clone_old: bool,
        path: &'static str,
    ) -> Result<(), PalwStateV2Error>
    where
        K: Ord + NativeHeapV1,
        V: NativeHeapV1,
    {
        if !self.native_writers_tracking_v1() {
            return Ok(());
        }
        let mut q = NativeWriteQuoteV1::new(path);
        {
            let rows = map(&self.state);
            let old = rows.get(key);
            q.copy(new)?.copy(new)?.copy(key)?.copy(key)?.member_if::<K, V>(old.is_none() && new.is_some())?.slots(1)?;
            if clone_old && let Some(old) = old {
                q.copy(old)?;
            }
        }
        self.admit_native_write_v1(q)
    }

    /// Journal slots only (a Copy scalar writer whose value owns no heap, or a note journal of a fixed-size entry).
    pub(super) fn reserve_journal_slots_v1(&mut self, slots: usize, path: &'static str) -> Result<(), PalwStateV2Error> {
        if !self.native_writers_tracking_v1() {
            return Ok(());
        }
        let mut q = NativeWriteQuoteV1::new(path);
        q.slots(slots)?;
        self.admit_native_write_v1(q)
    }

    /// A scalar / singleton writer: only its journal slot(s) plus the new value's two copies (field and journal).
    pub(super) fn reserve_scalar_write_v1<V: NativeHeapV1>(
        &mut self,
        new: &V,
        slots: usize,
        path: &'static str,
    ) -> Result<(), PalwStateV2Error> {
        if !self.native_writers_tracking_v1() {
            return Ok(());
        }
        let mut q = NativeWriteQuoteV1::new(path);
        q.copy(new)?.copy(new)?.slots(slots)?;
        self.admit_native_write_v1(q)
    }

    /// `n` boxed copies of `V` a journal entry owns (`old.map(Box::new)`, `new.map(Box::new)`): the box allocations
    /// themselves; the boxed values' heap is already the row quote's copies.
    pub(super) fn reserve_boxes_v1<V>(&mut self, n: u64, path: &'static str) -> Result<(), PalwStateV2Error> {
        if !self.native_writers_tracking_v1() || n == 0 {
            return Ok(());
        }
        let mut q = NativeWriteQuoteV1::new(path);
        q.exact::<V>(n)?;
        self.admit_native_write_v1(q)
    }

    /// A derived-index-only writer (no journal entry): `n` new memberships of `BTreeMap<K, V>`.
    pub(super) fn reserve_index_members_v1<K, V>(&mut self, n: u64, path: &'static str) -> Result<(), PalwStateV2Error> {
        if !self.native_writers_tracking_v1() || n == 0 {
            return Ok(());
        }
        let mut q = NativeWriteQuoteV1::new(path);
        q.member::<K, V>(n)?;
        self.admit_native_write_v1(q)
    }

    /// **The generic encoded-row writers** (`write_vertex_row`, `write_improvement_row`): the typed map clone of `new`,
    /// the key clone, the absent membership, and the three canonical Borsh buffers the journal entry owns (key, old,
    /// new), each sized by its allocation-free canonical length.
    pub(super) fn reserve_encoded_row_v1<K, V>(
        &mut self,
        key: &K,
        new: &Option<V>,
        map: fn(&mut PalwChainStateV2) -> &mut BTreeMap<K, V>,
        path: &'static str,
    ) -> Result<(), PalwStateV2Error>
    where
        K: Ord + NativeHeapV1 + borsh::BorshSerialize,
        V: NativeHeapV1 + borsh::BorshSerialize,
    {
        if !self.native_writers_tracking_v1() {
            return Ok(());
        }
        let mut q = NativeWriteQuoteV1::new(path);
        {
            let rows = map(&mut self.state);
            let old = rows.get(key);
            q.copy(new)?.copy(new)?.copy(key)?.member_if::<K, V>(old.is_none() && new.is_some())?.encoded(key)?.slots(1)?;
            if let Some(row) = new {
                q.encoded(row)?;
            }
            if let Some(row) = old {
                q.encoded(row)?;
            }
        }
        self.admit_native_write_v1(q)
    }

    /// **The kernel route's row writer** (ledger tables below 32, aux tables from 32): the map's key clone and value
    /// clone, the transient lookup key used to tell an absent row, the absent `(u8, Vec<u8>)` membership, and the
    /// caller-owned key/value buffers the journal entry retains for the transition's lifetime. The latter were encoded
    /// by a legacy caller before this admission; [`Self::write_kernel_row_encoded_v1`] admits them before encoding.
    pub(super) fn reserve_kernel_row_write_v1(
        &mut self,
        table: u8,
        key: &[u8],
        new: &Option<Vec<u8>>,
        retained_caller_buffers: bool,
    ) -> Result<(), PalwStateV2Error> {
        // The writer is a no-op without a route header; it allocates nothing.
        if !self.native_writers_tracking_v1() || self.state.kernel_route.is_none() {
            return Ok(());
        }
        let path = "native kernel route row writer";
        let key_len = key.len() as u64;
        let value_len = new.as_ref().map_or(0, |v| v.len() as u64);
        let mut q = NativeWriteQuoteV1::new(path);
        // The absent test needs an owned `(u8, Vec<u8>)` lookup key: reserve it first, then look up.
        q.exact::<u8>(key_len)?;
        self.admit_native_write_v1(q)?;
        let absent = new.is_some()
            && self.state.kernel_route.as_ref().is_some_and(|kernel| {
                let map =
                    if table < crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_TABLE_BOND_KEYS_V1 { &kernel.rows } else { &kernel.aux };
                !map.contains_key(&(table, key.to_vec()))
            });
        let mut q = NativeWriteQuoteV1::new(path);
        // `(table, key.clone())` for the map, `bytes.clone()` for the map value, both exact-capacity clones.
        q.exact::<u8>(key_len)?.exact::<u8>(value_len)?.member_if::<(u8, Vec<u8>), Vec<u8>>(absent)?.slots(1)?;
        if retained_caller_buffers {
            q.exact::<u8>(key_len)?.exact::<u8>(value_len)?;
        }
        self.admit_native_write_v1(q)
    }

    /// **Typed kernel aux/ledger row write, admitted before encoding**: the key and the row's canonical Borsh buffer
    /// are quoted from the actual typed value (allocation-free canonical length) before either Vec exists, then the
    /// standard row writer runs with no further caller-buffer charge.
    pub(super) fn write_kernel_row_encoded_v1<T: borsh::BorshSerialize>(
        &mut self,
        table: u8,
        key: &[u8],
        row: Option<&T>,
    ) -> Result<(), PalwStateV2Error> {
        if self.native_writers_tracking_v1() {
            let mut q = NativeWriteQuoteV1::new("native kernel route typed row encode");
            q.exact::<u8>(key.len() as u64)?;
            if let Some(row) = row {
                q.encoded(row)?;
            }
            self.admit_native_write_v1(q)?;
        }
        let bytes = row.map(|row| borsh::to_vec(row).expect("a typed kernel row serializes"));
        self.write_kernel_row_admitted_v1(table, key.to_vec(), bytes)
    }
}
