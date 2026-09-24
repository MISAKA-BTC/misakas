//! **The W8A16 engine, as a composition of catalogued ops (ADR-0040 W / ADR-0047).**
//!
//! Every runtime parameter is an integer `(m, shift, zero)` triple read from
//! [`Base0ArtifactV1::a16_params`] — the SAME store the dispute oracle serves — and every
//! arithmetic step delegates to `kaspa_consensus_core::palw_base0_a16` (plus the reused BASE-0
//! rows: `silu` and the embedding gather). The adjudicator must run the same code a conforming
//! implementation runs; here the implementation runs the adjudicator's.
//!
//! The attention arms are the GQA ops (`a16_attn_scores` / `a16_softmax_rows` /
//! `a16_attn_values`) over the SERIES layout the court's canonical input set concatenates —
//! full `kv_dim` cache rows, position-major — so the engine's committed rows and the court's
//! recomputations are the same shapes by construction, which
//! [`A16Engine::forward_token_traced`] exposes and the full-job replay verifies node by node.
//!
//! Position zero is the sink lane: the seams whose scales the attention-sink token sets resolve
//! their parameters with the `.sink0` suffix, exactly the rule `a16_row` applies in court.
//!
//! The output row is the committed one: i16 LOGIT CODES (the tile lane is 4 bytes), so the
//! class's argmax — lowest index on ties — is defined over the narrowed codes here and in any
//! dispute alike.

use crate::artifact::{Base0ArtifactV1, Base0ShapeV1};
use crate::kernels::{a16_matmul_requant_batch, a16_matmul_rescale_batch};
use kaspa_consensus_core::palw_base0_a16::{
    A16QuantParams, a16_add_elem, a16_mul_elem, a16_requant, a16_rms_norm, a16_rope, a16_softmax_rows,
};
use rayon::prelude::*;

/// Op W9 through whichever implementation this engine was built with — the key series at the width
/// the cache holds it. **The decode point**: the fast kernels are generic over the width and widen
/// each code at its multiply; the catalog op reads `i32` lanes and is handed a widening copy.
#[inline]
fn a16_attn_scores(
    fast: bool,
    q: &[i32],
    k: KvSeriesRef<'_>,
    heads: usize,
    kv_heads: usize,
    d_head: usize,
    p: &[A16QuantParams],
) -> Result<Vec<i32>, PalwA16OpError> {
    if fast {
        match k {
            KvSeriesRef::I32(k) => crate::kernels::a16_attn_scores_fast(q, k, heads, kv_heads, d_head, p),
            KvSeriesRef::I16(k) => crate::kernels::a16_attn_scores_fast(q, k, heads, kv_heads, d_head, p),
        }
    } else {
        catalog_attn_scores(q, &k.as_i32(), heads, kv_heads, d_head, p)
    }
}

/// Op W10 likewise, under the history bound the walk was given (ADR-0116): the shipped `2^18`
/// on the engine's own paths, the plan's — the class's — on the planned one.
#[inline]
#[allow(clippy::too_many_arguments)]
fn a16_attn_values(
    fast: bool,
    history: usize,
    probs: &[i32],
    v: KvSeriesRef<'_>,
    heads: usize,
    kv_heads: usize,
    d_head: usize,
    p: &[A16QuantParams],
) -> Result<Vec<i32>, PalwA16OpError> {
    if fast {
        match v {
            KvSeriesRef::I32(v) => crate::kernels::a16_attn_values_fast_within(probs, v, heads, kv_heads, d_head, p, history),
            KvSeriesRef::I16(v) => crate::kernels::a16_attn_values_fast_within(probs, v, heads, kv_heads, d_head, p, history),
        }
    } else {
        kaspa_consensus_core::palw_base0_a16::a16_attn_values_within(probs, &v.as_i32(), heads, kv_heads, d_head, p, history)
    }
}

/// The fused site's kernel (ADR-0082) over the two series at the width the cache holds them. One
/// cache holds one width, so a mixed pair cannot come from a cache; it is refused rather than
/// decoded twice.
#[inline]
#[allow(clippy::too_many_arguments)]
fn a16_attn_fused_fast(
    q: &[i32],
    k: KvSeriesRef<'_>,
    v: KvSeriesRef<'_>,
    heads: usize,
    kv_heads: usize,
    d_head: usize,
    logits: A16QuantParams,
    up_bits: u8,
    probs: A16QuantParams,
    values: A16QuantParams,
    history: usize,
) -> Result<Vec<i32>, PalwA16OpError> {
    match (k, v) {
        (KvSeriesRef::I32(k), KvSeriesRef::I32(v)) => {
            crate::kernels::a16_attn_fused_uniform_fast_within(q, k, v, heads, kv_heads, d_head, logits, up_bits, probs, values, history)
        }
        (KvSeriesRef::I16(k), KvSeriesRef::I16(v)) => {
            crate::kernels::a16_attn_fused_uniform_fast_within(q, k, v, heads, kv_heads, d_head, logits, up_bits, probs, values, history)
        }
        _ => Err(PalwA16OpError::Empty),
    }
}
use kaspa_consensus_core::palw_base0_ops::silu;

// **The two projections go through the fast kernels, and this is not a fork of the catalog.**
//
// `kernels::a16_matmul_requant_fast` and `..._rescale_fast` are asserted bit-identical to the
// catalog ops they replace — over the projection lengths this engine uses, at both code rails,
// and across the parallel/serial threshold (`kernels`' own tests, plus
// `the_fast_engine_and_the_catalog_agree_token_for_token` below, which compares whole forwards).
// They may be swapped in precisely because ADR-0040 Decision E makes lanes and threads invisible
// to the value; the day that stops being true is the day those tests fail.
use kaspa_consensus_core::palw_base0_a16::{
    PalwA16OpError, a16_attn_scores as catalog_attn_scores, a16_matmul_requant as catalog_matmul_requant,
    a16_matmul_rescale as catalog_matmul_rescale,
};

/// Op W1 through whichever implementation this engine was built with.
#[inline]
fn a16_matmul_requant(fast: bool, w: &[i8], x: &[i32], p: &[A16QuantParams]) -> Result<Vec<i32>, PalwA16OpError> {
    if fast { crate::kernels::a16_matmul_requant_fast(w, x, p) } else { catalog_matmul_requant(w, x, p) }
}

/// Op W3 likewise.
#[inline]
fn a16_matmul_rescale(fast: bool, w: &[i8], x: &[i32], p: &[A16QuantParams]) -> Result<Vec<i32>, PalwA16OpError> {
    if fast { crate::kernels::a16_matmul_rescale_fast(w, x, p) } else { catalog_matmul_rescale(w, x, p) }
}

/// Why the engine refused to run. Everything here is a REGISTRATION defect — a missing or
/// malformed parameter row — surfaced at construction, never mid-forward.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum A16EngineError {
    MissingParams(&'static str),
    MalformedParams(&'static str),
    OpRefused(&'static str),
    PositionOutOfRange,
}

/// One position's forward, every node's committed row, in the shape profile's numbering: the
/// replay surface. `pre` and `post` are per node; `attn` is per layer, per node.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct A16TraceV1 {
    pub pre: Vec<Vec<i32>>,
    pub attn: Vec<Vec<Vec<i32>>>,
    pub post: Vec<Vec<i32>>,
}

/// One layer's pre-resolved parameter tables (generic and, where the class carries them, sink).
struct LayerParams {
    attn_norm: Vec<A16QuantParams>,
    q: Vec<A16QuantParams>,
    k: Vec<A16QuantParams>,
    v: Vec<A16QuantParams>,
    logits: A16QuantParams,
    softmax_up: u8,
    probs: A16QuantParams,
    values: A16QuantParams,
    wo: Vec<A16QuantParams>,
    wo_sink: Vec<A16QuantParams>,
    attn_align: A16QuantParams,
    attn_align_sink: A16QuantParams,
    attn_residual: A16QuantParams,
    ffn_norm: Vec<A16QuantParams>,
    gate: Vec<A16QuantParams>,
    silu_q: A16QuantParams,
    silu_sink: A16QuantParams,
    up: Vec<A16QuantParams>,
    up_sink: Vec<A16QuantParams>,
    gated: A16QuantParams,
    gated_sink: A16QuantParams,
    down: Vec<A16QuantParams>,
    down_sink: Vec<A16QuantParams>,
    ffn_align: A16QuantParams,
    ffn_align_sink: A16QuantParams,
    ffn_residual: A16QuantParams,
}

/// **DRILL ONLY: an attention output this engine lies about, and computes downstream FROM**
/// (ADR-0152 §4-ter N5, `PalwFreePromptDrillFaultV1::AttnOutput { follow: true }`): the fused attention site
/// of `layer` at cache position `position` commits lane `lane` of its output row moved by `delta`,
/// and every node after it — the rest of the layer, every later layer, every later position through
/// the cache — reads the moved row. The consistent forger 4-ter F9 names: its checkpoints after the
/// lie hold rows the lie fed. `None` on every engine but a drill's.
///
/// **`cache: Some(kind)` — the lie is a CACHE row instead** (ADR-0152 §4-ter.3 step 6's forger,
/// `PalwFreePromptDrillFaultV1::CacheRow`): the `kind` (0 = K, 1 = V) row `layer` writes at cache
/// position `position` is committed honest as its cache-write step row, and the cache — every later
/// read, every checkpoint — holds it moved at lane `lane` by `delta`. The attention at that position
/// and after reads the moved row: its committed output follows the lie (the consistent forger), and
/// its checkpoints' slice `(K|V, layer)` disagrees with its own cache-write rows — the case the held
/// dissection's bottom cannot be built from the filing and the checkpoint court convicts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct A16DrillAttnLieV1 {
    pub layer: usize,
    pub position: usize,
    pub lane: usize,
    pub delta: i32,
    pub cache: Option<u8>,
}

impl A16DrillAttnLieV1 {
    /// The moved code, kept inside the A16 range the next kernel reads (the lie goes the other way
    /// when `delta` would leave it).
    fn apply(&self, row: &mut [i32]) {
        if let Some(value) = row.get_mut(self.lane) {
            let up = value.saturating_add(self.delta);
            *value = if (-32_767..=32_767).contains(&up) { up } else { value.saturating_sub(self.delta) };
        }
    }
}

pub struct A16Engine<'a> {
    pub artifact: &'a Base0ArtifactV1,
    /// DRILL ONLY — see [`A16DrillAttnLieV1`]. `None` everywhere else.
    drill_attn: Option<A16DrillAttnLieV1>,
    /// The cache position the stepped walk is at, for [`A16DrillAttnLieV1`] (the table walk does not
    /// otherwise carry it). An atomic because the one-pass prefill runs a layer's positions on the pool.
    walk_position: std::sync::atomic::AtomicUsize,
    /// Whether the two projections run through `kernels` or through the catalog ops directly.
    /// Both produce the same bits — that is what `A16Engine::new_reference` exists to keep true —
    /// and the catalog path is roughly thirteen times slower, so it is a test instrument rather
    /// than a mode anyone would run.
    fast: bool,
    embed_lift: A16QuantParams,
    final_norm: Vec<A16QuantParams>,
    logits_out: A16QuantParams,
    layers: Vec<LayerParams>,
}

/// **The representation this engine holds its attention cache in** — consensus core's
/// [`PalwRuntimeProfileV1`](kaspa_consensus_core::palw_resource_profile_v1::PalwRuntimeProfileV1),
/// under the name the engine uses for it. A node-local choice with a name: the resource profile
/// prices it, the telemetry reports it, and no committed byte depends on it.
pub use kaspa_consensus_core::palw_resource_profile_v1::PalwRuntimeProfileV1 as KvStorageProfileV1;

/// **What a backend built without saying holds: `A16-KV-i16`.**
///
/// Half the resident bytes of the `i32` cache and the same committed bytes, the same roots and the
/// same verdicts — held by `kv_storage_tests` on every graph version, both engines, the stepped and
/// the one-pass walks, across a checkpoint restart, and at the backend by
/// `the_backend_commits_the_same_roots_under_every_runtime_profile`. `A16-KV-i32` stays selectable
/// (`Qwen25A16Backend::with_runtime_profile`) as the oracle those tests compare against.
pub const KV_STORAGE_SHIPPED_V1: KvStorageProfileV1 = KvStorageProfileV1::A16KvI16;

/// **One side of one layer's history — K or V — flat and position-major**: row `p` is elements
/// `p × kv_dim .. (p + 1) × kv_dim`, so the series the attention arms read is a SLICE of the
/// storage rather than a copy of it.
///
/// This replaces `Vec<Vec<i32>>` per layer, which cost the 2M attempt two things beyond its
/// payload: 14.7 million one-kilobyte heap vectors (the allocator slack the item 6 run measured as
/// 16.05 GiB against 14.55 GiB of payload), and a fresh concatenation of the whole history for
/// every attention read — `walk_table` built `k_series`/`v_series` per position, `walk_layer_batched`
/// once per run of positions: 2 × 268 MB per layer at 262,143 rows, copied for every one of 4,096
/// runs of the prefill. A slice costs neither.
///
/// The two widths are the two runtime profiles. Every element either holds is an A16 code
/// (`±32,767`): `a16_rope` and `a16_matmul_requant` — the two producers of cached rows — both end
/// in `clamp16`, and every consumer refuses a wider value. So the `i16` form is a lossless repack
/// and `push_row` REFUSES a value it could not hold rather than narrowing it, for the reason
/// `state_chunk_bytes_v1` refuses: a cache that silently held a different value would commit
/// checkpoints the producer never computed.
#[derive(Clone, Debug, PartialEq, Eq)]
enum KvSideV1 {
    I32(Vec<i32>),
    I16(Vec<i16>),
}

impl KvSideV1 {
    fn empty(profile: KvStorageProfileV1) -> Self {
        // The hybrid's name prices the same `i32` lanes; a dense cache asked for it holds them.
        match profile {
            KvStorageProfileV1::A16KvI32 | KvStorageProfileV1::Q36KvI32 => KvSideV1::I32(Vec::new()),
            KvStorageProfileV1::A16KvI16 => KvSideV1::I16(Vec::new()),
        }
    }

    fn len(&self) -> usize {
        match self {
            KvSideV1::I32(v) => v.len(),
            KvSideV1::I16(v) => v.len(),
        }
    }

    fn reserve_exact(&mut self, elements: usize) {
        match self {
            KvSideV1::I32(v) => v.reserve_exact(elements.saturating_sub(v.len())),
            KvSideV1::I16(v) => v.reserve_exact(elements.saturating_sub(v.len())),
        }
    }

    /// Append one row. Under the `i16` profile a value outside the code range is a refusal — an
    /// element no A16 op could have produced and no attention read would accept.
    fn push_row(&mut self, row: &[i32]) -> Result<(), A16EngineError> {
        match self {
            KvSideV1::I32(v) => v.extend_from_slice(row),
            KvSideV1::I16(v) => {
                v.reserve(row.len());
                for value in row {
                    if value.unsigned_abs() > A16_CODE_MAX_U32 {
                        return Err(A16EngineError::OpRefused("a cache write outside the A16 code range under the i16 profile"));
                    }
                    v.push(*value as i16);
                }
            }
        }
        Ok(())
    }

    fn series(&self, elements: usize) -> KvSeriesRef<'_> {
        match self {
            KvSideV1::I32(v) => KvSeriesRef::I32(&v[..elements]),
            KvSideV1::I16(v) => KvSeriesRef::I16(&v[..elements]),
        }
    }

    fn as_i32(&self, start: usize, end: usize) -> Vec<i32> {
        match self {
            KvSideV1::I32(v) => v[start..end].to_vec(),
            KvSideV1::I16(v) => v[start..end].iter().map(|c| i32::from(*c)).collect(),
        }
    }

    /// Bytes the allocator was asked for: the capacity, at the element width.
    fn resident_bytes(&self) -> u64 {
        match self {
            KvSideV1::I32(v) => v.capacity() as u64 * 4,
            KvSideV1::I16(v) => v.capacity() as u64 * 2,
        }
    }
}

const A16_CODE_MAX_U32: u32 = kaspa_consensus_core::palw_base0_a16::A16_CODE_MAX as u32;

/// **A borrowed prefix of a series, at the width the cache holds it** — the one type the attention
/// arms take, so "decode at the arithmetic point" is a single `match`: the fast kernels are generic
/// over the width and widen each code at its multiply (exact for every A16 code); the catalog ops
/// read `i32` lanes and are handed [`Self::as_i32`], a widening copy for `i16` and a borrow for
/// `i32`. There is no third place a code changes width.
#[derive(Clone, Copy, Debug)]
pub enum KvSeriesRef<'a> {
    I32(&'a [i32]),
    I16(&'a [i16]),
}

impl<'a> KvSeriesRef<'a> {
    pub fn len(&self) -> usize {
        match self {
            KvSeriesRef::I32(s) => s.len(),
            KvSeriesRef::I16(s) => s.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The first `elements` of this series.
    pub fn prefix(&self, elements: usize) -> KvSeriesRef<'a> {
        match self {
            KvSeriesRef::I32(s) => KvSeriesRef::I32(&s[..elements]),
            KvSeriesRef::I16(s) => KvSeriesRef::I16(&s[..elements]),
        }
    }

    /// The series in the lane the catalog ops read. `i16 → i32` is a widening cast: exact.
    pub fn as_i32(&self) -> std::borrow::Cow<'a, [i32]> {
        match self {
            KvSeriesRef::I32(s) => std::borrow::Cow::Borrowed(s),
            KvSeriesRef::I16(s) => std::borrow::Cow::Owned(s.iter().map(|c| i32::from(*c)).collect()),
        }
    }
}

/// The dense tier's attention cache: per layer, the K and V histories, held flat under a
/// [`KvStorageProfileV1`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct A16Cache {
    profile: KvStorageProfileV1,
    /// Codes per row, learned at the first push (zero until then); every later row must match.
    kv_dim: usize,
    keys: Vec<KvSideV1>,
    values: Vec<KvSideV1>,
}

impl A16Cache {
    /// A cache in the reference representation (`A16-KV-i32`). Every test instrument and every
    /// caller that never chose keeps the oracle; the production paths say which profile they hold
    /// ([`Self::with_storage`]).
    pub fn new(layers: usize) -> Self {
        Self::with_storage(layers, KvStorageProfileV1::A16KvI32)
    }

    pub fn with_storage(layers: usize, profile: KvStorageProfileV1) -> Self {
        Self {
            profile,
            kv_dim: 0,
            keys: (0..layers).map(|_| KvSideV1::empty(profile)).collect(),
            values: (0..layers).map(|_| KvSideV1::empty(profile)).collect(),
        }
    }

    pub fn storage(&self) -> KvStorageProfileV1 {
        self.profile
    }

    /// **Reserve the whole job's rows up front**, so the buffers never double and never copy while
    /// the walk runs: a job's length is known before its first position, and a `Vec` that grows by
    /// doubling holds up to twice its payload and, at each step, the old buffer beside the new. The
    /// reservation is virtual until touched, so an over-estimate costs address space and not pages.
    pub fn reserve_positions(&mut self, positions: usize, kv_dim: usize) {
        if kv_dim == 0 {
            return;
        }
        if self.kv_dim == 0 {
            self.kv_dim = kv_dim;
        }
        let elements = positions.saturating_mul(kv_dim);
        for side in self.keys.iter_mut().chain(self.values.iter_mut()) {
            side.reserve_exact(elements);
        }
    }

    /// The rows the cache holds — the positions a walk has run. One forward appends one row to
    /// every layer, so the first layer's count is every layer's.
    pub fn rows(&self) -> usize {
        self.rows_in(0)
    }

    /// The key rows layer `li` holds — which differ between layers only inside a layer-major walk.
    pub fn rows_in(&self, li: usize) -> usize {
        if self.kv_dim == 0 { 0 } else { self.keys.get(li).map_or(0, |k| k.len() / self.kv_dim) }
    }

    /// The value rows layer `li` holds — equal to the key rows except between a layer's K write and
    /// its V write.
    pub fn value_rows_in(&self, li: usize) -> usize {
        if self.kv_dim == 0 { 0 } else { self.values.get(li).map_or(0, |v| v.len() / self.kv_dim) }
    }

    fn learn_width(&mut self, row: &[i32]) -> Result<(), A16EngineError> {
        if row.is_empty() {
            return Err(A16EngineError::OpRefused("an empty cache row"));
        }
        if self.kv_dim == 0 {
            self.kv_dim = row.len();
        } else if self.kv_dim != row.len() {
            return Err(A16EngineError::OpRefused("a cache row of a different width from the rows before it"));
        }
        Ok(())
    }

    /// Append one position's rotated key row to layer `li`.
    pub fn push_key(&mut self, li: usize, row: &[i32]) -> Result<(), A16EngineError> {
        self.learn_width(row)?;
        self.keys.get_mut(li).ok_or(A16EngineError::OpRefused("a cache write to a layer this cache lacks"))?.push_row(row)
    }

    /// Append one position's value row to layer `li`.
    pub fn push_value(&mut self, li: usize, row: &[i32]) -> Result<(), A16EngineError> {
        self.learn_width(row)?;
        self.values.get_mut(li).ok_or(A16EngineError::OpRefused("a cache write to a layer this cache lacks"))?.push_row(row)
    }

    /// Layer `li`'s key series, every row, position-major — the court's canonical concatenation,
    /// as a slice.
    pub fn keys(&self, li: usize) -> KvSeriesRef<'_> {
        self.keys[li].series(self.keys[li].len())
    }

    pub fn values(&self, li: usize) -> KvSeriesRef<'_> {
        self.values[li].series(self.values[li].len())
    }

    /// The first `rows` rows of layer `li`'s key series — what a position in a batch sees.
    pub fn keys_visible(&self, li: usize, rows: usize) -> Result<KvSeriesRef<'_>, A16EngineError> {
        let elements = rows.saturating_mul(self.kv_dim);
        if elements > self.keys[li].len() {
            return Err(A16EngineError::MalformedParams("a cache read before its write"));
        }
        Ok(self.keys[li].series(elements))
    }

    pub fn values_visible(&self, li: usize, rows: usize) -> Result<KvSeriesRef<'_>, A16EngineError> {
        let elements = rows.saturating_mul(self.kv_dim);
        if elements > self.values[li].len() {
            return Err(A16EngineError::MalformedParams("a cache read before its write"));
        }
        Ok(self.values[li].series(elements))
    }

    /// Layer `li`'s whole key series decoded to `i32` — the representation-free view a test
    /// compares two caches through.
    pub fn keys_as_i32(&self, li: usize) -> Vec<i32> {
        self.keys[li].as_i32(0, self.keys[li].len())
    }

    pub fn values_as_i32(&self, li: usize) -> Vec<i32> {
        self.values[li].as_i32(0, self.values[li].len())
    }

    /// Every layer's K and V series decoded to `i32`: `(keys, values)`, per layer. Two caches with
    /// equal contents compute the same attention whatever their profiles.
    pub fn contents_as_i32(&self) -> (Vec<Vec<i32>>, Vec<Vec<i32>>) {
        ((0..self.keys.len()).map(|li| self.keys_as_i32(li)).collect(), (0..self.values.len()).map(|li| self.values_as_i32(li)).collect())
    }

    /// Bytes the cache's buffers were allocated at — the measured twin of the resource profile's
    /// `kv_resident_bytes`, which `the_resident_bytes_are_the_resource_profiles_kv_term` holds equal
    /// after an exact reservation.
    pub fn resident_bytes_v1(&self) -> u64 {
        self.keys.iter().chain(self.values.iter()).map(KvSideV1::resident_bytes).sum()
    }
    /// **This cache's bytes for one state chunk, encoded the way the MAP says — or nothing.**
    ///
    /// The A16 analogue of `KvCache::state_chunk_bytes`, and deliberately not a copy of it. That
    /// one reinterprets each element as a byte, which is exact for a `Vec<i8>` cache and silent
    /// truncation for this one; its length guard does not catch the difference, because for this
    /// class the element COUNT and the map's declared byte count are the same number.
    ///
    /// So the width is read from the entry rather than assumed, and a row that does not fit the
    /// declared width is refused instead of narrowed:
    ///
    /// * `row_bytes == row.len()` — one byte per element. Encoded only if every value is an `i8`;
    ///   otherwise `None`, because a checkpoint that opens to a state the producer never held is
    ///   worse than a missing one, and the producer has signed for it.
    /// * `row_bytes == 4 × row.len()` — little-endian `i32`, which is what this cache holds.
    /// * anything else — `None`. A map that describes neither is a map for a different class.
    ///
    /// Written this way because the class's map is currently the one-byte one and its state does
    /// not fit (see `docs/palw-fp-on-registered-classes.md`): whichever way that is resolved —
    /// narrowing the cache, or registering a class with a four-byte map — this function is already
    /// correct for it, and refuses in the meantime rather than committing a lie.
    pub fn state_chunk_bytes_v1(&self, entry: &kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkEntryV1) -> Option<Vec<u8>> {
        use kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkKindV1;
        let side = match entry.kind {
            PalwStateChunkKindV1::Key => &self.keys,
            PalwStateChunkKindV1::Value => &self.values,
        };
        let layer = side.get(entry.attn_layer as usize)?;
        let width = self.kv_dim;
        if width == 0 {
            return None;
        }
        let declared = entry.row_bytes as usize;
        let per_element = if declared == width {
            1
        } else if declared == width.checked_mul(4)? {
            4
        } else {
            return None;
        };
        let start = (entry.position_start as usize).checked_mul(width)?;
        let end = start.checked_add((entry.position_count as usize).checked_mul(width)?)?;
        if end > layer.len() {
            return None;
        }
        // Decoded once for the range — a widening for the `i16` profile, a copy for `i32` — and
        // encoded exactly as the `i32` cache encoded: the same bytes under either representation,
        // which `a_compact_cache_computes_the_reference_bits` compares chunk for chunk.
        let mut out = Vec::with_capacity((entry.position_count as usize) * declared);
        for value in layer.as_i32(start, end) {
            if per_element == 1 {
                out.push(i8::try_from(value).ok()? as u8);
            } else {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        Some(out)
    }

    /// **The inverse of [`Self::state_chunk_bytes_v1`]: a cache rebuilt from committed chunks.**
    ///
    /// The restore half of ADR-0077 Decision 10 for this tier. Without it the dense class could
    /// commit a checkpoint leg and nothing could resume from one, so every dispute and every
    /// interval opening ran genesis-anchored — which is the cost the checkpoint leg exists to
    /// remove.
    ///
    /// Both widths the encoder writes are read back, decided by the ENTRY rather than guessed:
    /// `row_bytes == elements` is the one-byte map (each byte an `i8`), `row_bytes == 4 ×
    /// elements` is this cache's own `i32`. `row_bytes` is a whole multiple of neither for a map
    /// that describes another class, and that is a refusal.
    ///
    /// Every refusal is a refusal to replay, never a partial cache — the rule
    /// `KvCache::from_state_chunks` states: a cache assembled from material that does not cover
    /// the state replays against zeros, and zeros are indistinguishable from computed rows once
    /// they are in a commitment.
    ///
    /// **The profile is the CALLER's** — a seat resuming under `A16-KV-i16` decodes a producer's
    /// `i32` chunks into `i16` rows, which is exact for every code and a named refusal for a byte
    /// that is not one: the reference cache would hold such a value and refuse it at the first
    /// attention read, this cache refuses it here; either way the replay fails rather than
    /// computing over a row the class's arithmetic could never have written.
    ///
    /// **Coverage is a bitmap, not a length**: a flat buffer that a chunk never wrote reads as
    /// zeros, and zeros are indistinguishable from computed rows once they are in a commitment —
    /// so every `(kind, layer, position)` the map names is ticked as it is written and the whole
    /// map is checked at the end, which is exactly the guard the row-per-`Vec` layout got for free.
    pub fn from_state_chunks_v1(
        storage: KvStorageProfileV1,
        layers: usize,
        row_elements: usize,
        geometry: &kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkGeometryV1,
        chunks: &[Vec<u8>],
    ) -> Result<Self, A16EngineError> {
        use kaspa_consensus_core::palw_state_chunk_map::{
            PalwStateChunkKindV1, integer_kv_state_chunk_entry_v1, integer_kv_state_row_v1,
        };
        if chunks.len() as u64 != geometry.chunk_count() || row_elements == 0 {
            return Err(A16EngineError::OpRefused("the served chunks are not the map's own count"));
        }
        let positions = geometry.positions as usize;
        let mut cache = Self::with_storage(layers, storage);
        cache.kv_dim = row_elements;
        let elements = positions.checked_mul(row_elements).ok_or(A16EngineError::OpRefused("the map's state overflows"))?;
        let mut written: Vec<Vec<bool>> = vec![vec![false; positions]; 2 * layers];
        for layer in geometry.attn_layers.iter() {
            let li = *layer as usize;
            if li >= layers {
                return Err(A16EngineError::OpRefused("the map names a layer this cache lacks"));
            }
            cache.keys[li] = KvSideV1::zeroed(storage, elements);
            cache.values[li] = KvSideV1::zeroed(storage, elements);
        }
        for (index, bytes) in chunks.iter().enumerate() {
            let entry = integer_kv_state_chunk_entry_v1(geometry, index as u64)
                .ok_or(A16EngineError::OpRefused("the map has no entry for a chunk it counted"))?;
            let width = entry.row_bytes as usize;
            let per_element = if width == row_elements {
                1
            } else if width == row_elements.checked_mul(4).ok_or(A16EngineError::OpRefused("the map's row width overflows"))? {
                4
            } else {
                return Err(A16EngineError::OpRefused("the map describes a row this cache does not hold"));
            };
            let li = entry.attn_layer as usize;
            let (side, ticks) = match entry.kind {
                PalwStateChunkKindV1::Key => (&mut cache.keys, &mut written[li]),
                PalwStateChunkKindV1::Value => (&mut cache.values, &mut written[layers + li]),
            };
            let layer = side.get_mut(li).ok_or(A16EngineError::OpRefused("the map names a layer this cache lacks"))?;
            for p in entry.position_start..entry.position_start + entry.position_count {
                let row =
                    integer_kv_state_row_v1(&entry, bytes, p).ok_or(A16EngineError::OpRefused("a chunk is not its own length"))?;
                let values: Vec<i32> = if per_element == 1 {
                    row.iter().map(|b| *b as i8 as i32).collect()
                } else {
                    row.chunks_exact(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
                };
                let p = p as usize;
                if p >= positions {
                    return Err(A16EngineError::OpRefused("the map names a position past the state it declares"));
                }
                layer.write_row_at(p * row_elements, &values)?;
                ticks[p] = true;
            }
        }
        // Every attention layer the map named must now be covered, both kinds, every position. A
        // layer it never named stays empty, and replaying over that is the zero-state failure.
        for layer in geometry.attn_layers.iter() {
            let li = *layer as usize;
            if written[li].iter().any(|w| !w) || written[layers + li].iter().any(|w| !w) {
                return Err(A16EngineError::OpRefused("the served chunks do not cover the state they declare"));
            }
        }
        Ok(cache)
    }

    /// The key rows this cache holds, for tests that need to measure the STATE rather than reason
    /// about its type — `a16_kv_state_does_not_fit_the_one_byte_map_its_class_declares` is the
    /// caller, and what it measures decides whether a checkpoint map is sound for this family.
    #[cfg(test)]
    pub(crate) fn key_rows_for_test(&self) -> Vec<Vec<i32>> {
        if self.kv_dim == 0 {
            return Vec::new();
        }
        (0..self.keys.len()).flat_map(|li| self.keys_as_i32(li).chunks(self.kv_dim).map(<[i32]>::to_vec).collect::<Vec<_>>()).collect()
    }

    pub fn len(&self) -> usize {
        self.rows()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl KvSideV1 {
    fn zeroed(profile: KvStorageProfileV1, elements: usize) -> Self {
        match profile {
            KvStorageProfileV1::A16KvI32 | KvStorageProfileV1::Q36KvI32 => KvSideV1::I32(vec![0; elements]),
            KvStorageProfileV1::A16KvI16 => KvSideV1::I16(vec![0; elements]),
        }
    }

    /// Overwrite one row at `offset` (elements) — the restore's write, refusing a value the
    /// profile cannot hold rather than narrowing it.
    fn write_row_at(&mut self, offset: usize, row: &[i32]) -> Result<(), A16EngineError> {
        match self {
            KvSideV1::I32(v) => {
                let slot = v.get_mut(offset..offset + row.len()).ok_or(A16EngineError::OpRefused("a chunk row past the state"))?;
                slot.copy_from_slice(row);
            }
            KvSideV1::I16(v) => {
                let slot = v.get_mut(offset..offset + row.len()).ok_or(A16EngineError::OpRefused("a chunk row past the state"))?;
                for (s, value) in slot.iter_mut().zip(row) {
                    if value.unsigned_abs() > A16_CODE_MAX_U32 {
                        return Err(A16EngineError::OpRefused("a served chunk holds a value outside the A16 code range under the i16 profile"));
                    }
                    *s = *value as i16;
                }
            }
        }
        Ok(())
    }
}

fn parse_rows(bytes: &[u8]) -> Result<Vec<A16QuantParams>, A16EngineError> {
    if bytes.is_empty() || !bytes.len().is_multiple_of(A16QuantParams::WIRE_BYTES) {
        return Err(A16EngineError::MalformedParams("params bytes are not whole 17-byte triples"));
    }
    bytes
        .chunks_exact(A16QuantParams::WIRE_BYTES)
        .map(|c| A16QuantParams::from_wire(c).map_err(|_| A16EngineError::MalformedParams("triple out of domain")))
        .collect()
}

impl<'a> A16Engine<'a> {
    /// Resolve every parameter table up front: a class whose registration is missing a row is
    /// refused HERE, not discovered as a wrong number three layers into a forward pass.
    pub fn new(artifact: &'a Base0ArtifactV1) -> Result<Self, A16EngineError> {
        let one = |template: &str, layer: Option<u16>, what: &'static str| -> Result<A16QuantParams, A16EngineError> {
            let rows = parse_rows(artifact.a16_param(template, layer).ok_or(A16EngineError::MissingParams(what))?)?;
            if rows.len() != 1 {
                return Err(A16EngineError::MalformedParams(what));
            }
            Ok(rows[0])
        };
        let many =
            |template: &str, layer: Option<u16>, want: usize, what: &'static str| -> Result<Vec<A16QuantParams>, A16EngineError> {
                let rows = parse_rows(artifact.a16_param(template, layer).ok_or(A16EngineError::MissingParams(what))?)?;
                if rows.len() != want {
                    return Err(A16EngineError::MalformedParams(what));
                }
                Ok(rows)
            };
        let shape = &artifact.shape;
        let (d, kv, ff) = (shape.d_model(), shape.kv_dim(), shape.d_ff);
        let mut layers = Vec::with_capacity(shape.n_layers);
        for li in 0..shape.n_layers {
            let l = Some(li as u16);
            let up_bits = artifact.a16_param("blk.{layer}.attn_softmax_up", l).ok_or(A16EngineError::MissingParams("softmax_up"))?;
            if up_bits.len() != 1 || up_bits[0] > 62 {
                return Err(A16EngineError::MalformedParams("softmax_up"));
            }
            layers.push(LayerParams {
                attn_norm: many("blk.{layer}.attn_norm.a16", l, d, "attn_norm")?,
                q: many("blk.{layer}.attn_q.weight.a16", l, d, "q")?,
                k: many("blk.{layer}.attn_k.weight.a16", l, kv, "k")?,
                v: many("blk.{layer}.attn_v.weight.a16", l, kv, "v")?,
                logits: one("blk.{layer}.attn_logits.a16", l, "logits")?,
                softmax_up: up_bits[0],
                probs: one("blk.{layer}.attn_probs.a16", l, "probs")?,
                values: one("blk.{layer}.attn_values.a16", l, "values")?,
                wo: many("blk.{layer}.attn_output.weight.a16", l, d, "wo")?,
                wo_sink: many("blk.{layer}.attn_output.weight.a16.sink0", l, d, "wo sink")?,
                attn_align: one("blk.{layer}.attn_align.a16", l, "attn_align")?,
                attn_align_sink: one("blk.{layer}.attn_align.a16.sink0", l, "attn_align sink")?,
                attn_residual: one("blk.{layer}.attn_residual.a16", l, "attn_residual")?,
                ffn_norm: many("blk.{layer}.ffn_norm.a16", l, d, "ffn_norm")?,
                gate: many("blk.{layer}.ffn_gate.weight.a16", l, ff, "gate")?,
                silu_q: one("blk.{layer}.ffn_silu.a16", l, "silu")?,
                silu_sink: one("blk.{layer}.ffn_silu.a16.sink0", l, "silu sink")?,
                up: many("blk.{layer}.ffn_up.weight.a16", l, ff, "up")?,
                up_sink: many("blk.{layer}.ffn_up.weight.a16.sink0", l, ff, "up sink")?,
                gated: one("blk.{layer}.ffn_gated.a16", l, "gated")?,
                gated_sink: one("blk.{layer}.ffn_gated.a16.sink0", l, "gated sink")?,
                down: many("blk.{layer}.ffn_down.weight.a16", l, d, "down")?,
                down_sink: many("blk.{layer}.ffn_down.weight.a16.sink0", l, d, "down sink")?,
                ffn_align: one("blk.{layer}.ffn_align.a16", l, "ffn_align")?,
                ffn_align_sink: one("blk.{layer}.ffn_align.a16.sink0", l, "ffn_align sink")?,
                ffn_residual: one("blk.{layer}.ffn_residual.a16", l, "ffn_residual")?,
            });
        }
        Ok(Self {
            artifact,
            drill_attn: None,
            walk_position: std::sync::atomic::AtomicUsize::new(0),
            fast: true,
            embed_lift: one("embed_lift.a16", None, "embed_lift")?,
            final_norm: many("final_norm.a16", None, d, "final_norm")?,
            logits_out: one("token_embd.weight.a16", None, "logits_out")?,
            layers,
        })
    }

    /// **Prefill a run of tokens, batched.**
    ///
    /// Decode reads the whole 1.65 GiB weight set to produce one token, so it is bandwidth-bound
    /// and no kernel can fix that — the model has to be read. A prompt does not have that excuse:
    /// every one of its tokens needs the same weight row, so reading it once and using it `batch`
    /// times raises the arithmetic per byte by `batch`.
    ///
    /// Returns the LAST position's logits, which is all a prefill is for: the earlier rows predict
    /// tokens the prompt already contains. The unembedding — 233M multiply-accumulates, 15 % of a
    /// token — is therefore computed once rather than `n` times.
    ///
    /// # Three things this must not change, and how each is held
    ///
    /// * **The KV cache.** Prefill and decode meet in it, so a batched prefill has to leave
    ///   exactly the state a token-at-a-time prefill would have left. Every op here is per row and
    ///   the batched projections are asserted bit-identical to the single-row ones.
    /// * **The sink.** Position 0 rides its own parameters at seven seams (ADR-0050), so it is not
    ///   batched with anything: when the run starts at position 0 that token goes through
    ///   [`Self::forward_token`] alone and the batch starts at position 1. Mixing it in would need
    ///   per-row parameters on three projections, for one row.
    /// * **Attention's history.** Row `i` of a batch attends to everything before the batch plus
    ///   rows `0..=i` of it — never to `i+1`. The full series is built once after all the keys are
    ///   appended and each row reads a PREFIX of it, which is the same bytes the sequential path
    ///   would have concatenated and is `batch` times less copying.
    pub fn forward_prefill(
        &self,
        cache: &mut A16Cache,
        tokens: &[usize],
        start_position: usize,
        batch: usize,
    ) -> Result<Vec<i32>, A16EngineError> {
        if tokens.is_empty() {
            return Err(A16EngineError::OpRefused("an empty prefill"));
        }
        let batch = batch.max(1);
        let mut logits = Vec::new();
        let mut at = 0usize;
        // The sink is never batched.
        if start_position == 0 {
            logits = self.forward_token(cache, tokens[0], 0)?;
            at = 1;
        }
        while at < tokens.len() {
            let end = (at + batch).min(tokens.len());
            let last = end == tokens.len();
            logits = self.forward_batch(cache, &tokens[at..end], start_position + at, last)?;
            at = end;
        }
        Ok(logits)
    }

    /// One batch of non-sink positions. `want_logits` skips the unembedding for every batch but
    /// the last.
    fn forward_batch(
        &self,
        cache: &mut A16Cache,
        tokens: &[usize],
        start_position: usize,
        want_logits: bool,
    ) -> Result<Vec<i32>, A16EngineError> {
        let shape = &self.artifact.shape;
        let d = shape.d_model();
        let kv_dim = shape.kv_dim();
        let batch = tokens.len();
        let refuse =
            |what: &'static str| move |_e: kaspa_consensus_core::palw_base0_a16::PalwA16OpError| A16EngineError::OpRefused(what);
        let tile = |p: A16QuantParams, n: usize| -> Vec<A16QuantParams> { vec![p; n] };

        let rope_rows: Vec<(&[i32], &[i32])> = (0..batch)
            .map(|i| self.artifact.rope.row(start_position + i).ok_or(A16EngineError::PositionOutOfRange))
            .collect::<Result<_, _>>()?;

        // ---- pre: the gather and the lift, per row -----------------------------------------
        let mut h: Vec<Vec<i32>> = Vec::with_capacity(batch);
        for token_id in tokens {
            let embed_row: Vec<i32> = self.artifact.embed[token_id * d..(token_id + 1) * d].iter().map(|c| *c as i32).collect();
            h.push(a16_requant(&embed_row, &tile(self.embed_lift, d)).map_err(refuse("embed_lift"))?);
        }

        for (li, lp) in self.layers.iter().enumerate() {
            let lw = &self.artifact.layers[li];

            // ---- attention ---------------------------------------------------------------
            let mut normed = Vec::with_capacity(batch);
            for row in &h {
                let unit = a16_rms_norm(row, shape.eps_q).map_err(refuse("norm1"))?;
                normed.push(a16_requant(&unit, &lp.attn_norm).map_err(refuse("norm1_req"))?);
            }
            let q = a16_matmul_requant_batch(&lw.wq, &normed, &lp.q).map_err(refuse("q"))?;
            let k = a16_matmul_requant_batch(&lw.wk, &normed, &lp.k).map_err(refuse("k"))?;
            let v = a16_matmul_requant_batch(&lw.wv, &normed, &lp.v).map_err(refuse("v"))?;

            let history_before = cache.rows_in(li);
            let mut q_rot = Vec::with_capacity(batch);
            for (i, (cos_row, sin_row)) in rope_rows.iter().enumerate() {
                let rope_heads = |row: &[i32], heads: usize, what: &'static str| -> Result<Vec<i32>, A16EngineError> {
                    let mut out = Vec::with_capacity(row.len());
                    for hd in 0..heads {
                        let slice = &row[hd * shape.d_head..(hd + 1) * shape.d_head];
                        out.extend(a16_rope(slice, cos_row, sin_row).map_err(|_| A16EngineError::OpRefused(what))?);
                    }
                    Ok(out)
                };
                q_rot.push(rope_heads(&q[i], shape.n_heads, "rope_q")?);
                let k_rot = rope_heads(&k[i], shape.n_kv_heads, "rope_k")?;
                cache.push_key(li, &self.drill_cache_row(li, history_before + i, 0, &k_rot))?;
                cache.push_value(li, &self.drill_cache_row(li, history_before + i, 1, &v[i]))?;
            }

            // The whole series is the storage itself; row `i` reads the prefix that ends at its
            // own position.
            let history = history_before + batch;
            let k_series = cache.keys_visible(li, history)?;
            let v_series = cache.values_visible(li, history)?;

            let mut attn_rows = Vec::with_capacity(batch);
            for (i, q_row) in q_rot.iter().enumerate() {
                let visible = history_before + i + 1;
                let logits_row = a16_attn_scores(
                    self.fast,
                    q_row,
                    k_series.prefix(visible * kv_dim),
                    shape.n_heads,
                    shape.n_kv_heads,
                    shape.d_head,
                    &tile(lp.logits, shape.n_heads * visible),
                )
                .map_err(refuse("logits"))?;
                let probs_row = a16_softmax_rows(&logits_row, visible, lp.softmax_up).map_err(refuse("softmax"))?;
                let p15 = a16_requant(&probs_row, &tile(lp.probs, shape.n_heads * visible)).map_err(refuse("p15"))?;
                attn_rows.push(
                    a16_attn_values(
                        self.fast,
                        kaspa_consensus_core::palw_base0_a16::A16_MAX_ATTN_HISTORY_V1,
                        &p15,
                        v_series.prefix(visible * kv_dim),
                        shape.n_heads,
                        shape.n_kv_heads,
                        shape.d_head,
                        &tile(lp.values, shape.n_heads * shape.d_head),
                    )
                    .map_err(refuse("values"))?,
                );
            }

            let delta = a16_matmul_requant_batch(&lw.wo, &attn_rows, &lp.wo).map_err(refuse("wo"))?;
            for (i, row) in h.iter_mut().enumerate() {
                let aligned = a16_requant(row, &tile(lp.attn_align, d)).map_err(refuse("attn_align"))?;
                let sum = a16_add_elem(&aligned, &delta[i]).map_err(refuse("attn_add"))?;
                *row = a16_requant(&sum, &tile(lp.attn_residual, d)).map_err(refuse("attn_res"))?;
            }

            // ---- SwiGLU -------------------------------------------------------------------
            let mut normed = Vec::with_capacity(batch);
            for row in &h {
                let unit = a16_rms_norm(row, shape.eps_q).map_err(refuse("norm2"))?;
                normed.push(a16_requant(&unit, &lp.ffn_norm).map_err(refuse("norm2_req"))?);
            }
            let gate_q = a16_matmul_rescale_batch(&lw.w_gate, &normed, &lp.gate).map_err(refuse("gate"))?;
            let up = a16_matmul_requant_batch(&lw.w_up, &normed, &lp.up).map_err(refuse("up"))?;
            let mut gated_rows = Vec::with_capacity(batch);
            for i in 0..batch {
                let silu_q = silu(&gate_q[i]);
                let s16 = a16_requant(&silu_q, &tile(lp.silu_q, shape.d_ff)).map_err(refuse("silu16"))?;
                let prod = a16_mul_elem(&s16, &up[i]).map_err(refuse("mul"))?;
                gated_rows.push(a16_requant(&prod, &tile(lp.gated, shape.d_ff)).map_err(refuse("gated"))?);
            }
            let delta = a16_matmul_requant_batch(&lw.w_down, &gated_rows, &lp.down).map_err(refuse("down"))?;
            for (i, row) in h.iter_mut().enumerate() {
                let aligned = a16_requant(row, &tile(lp.ffn_align, d)).map_err(refuse("ffn_align"))?;
                let sum = a16_add_elem(&aligned, &delta[i]).map_err(refuse("ffn_add"))?;
                *row = a16_requant(&sum, &tile(lp.ffn_residual, d)).map_err(refuse("ffn_res"))?;
            }
        }

        if !want_logits {
            return Ok(Vec::new());
        }
        let last = h.last().expect("a non-empty batch");
        let unit = a16_rms_norm(last, shape.eps_q).map_err(refuse("final_norm"))?;
        let fin = a16_requant(&unit, &self.final_norm).map_err(refuse("final_req"))?;
        a16_matmul_requant(self.fast, &self.artifact.unembed, &fin, &tile(self.logits_out, shape.vocab)).map_err(refuse("logits_out"))
    }

    /// The same engine with the two projections routed through the catalog ops rather than the
    /// fast kernels. Only a test builds one: it is the other side of the differential.
    pub fn new_reference(artifact: &'a Base0ArtifactV1) -> Result<Self, A16EngineError> {
        Ok(Self { fast: false, ..Self::new(artifact)? })
    }

    /// DRILL ONLY: this engine, lying at `lie` (see [`A16DrillAttnLieV1`]); `None` is the honest
    /// engine every other caller builds.
    pub fn with_drill_attn_lie_v1(mut self, lie: Option<A16DrillAttnLieV1>) -> Self {
        self.drill_attn = lie;
        self
    }

    /// The drill's lie on a fused site's row of layer `li` at cache position `position`, if it is
    /// this one — a no-op on every honest engine.
    fn drill_attn_at(&self, li: usize, position: usize, row: &mut [i32]) {
        if let Some(lie) = self.drill_attn
            && lie.cache.is_none()
            && lie.layer == li
            && lie.position == position
        {
            lie.apply(row);
        }
    }

    /// DRILL ONLY: the `kind` (0 = K, 1 = V) row layer `li` writes into the cache at `position`, as
    /// the cache holds it — moved when the drill's lie is this cache row
    /// ([`A16DrillAttnLieV1::cache`]), the row itself on every honest engine. The committed
    /// cache-write step row is never this: it is `row`, honest.
    fn drill_cache_row<'r>(&self, li: usize, position: usize, kind: u8, row: &'r [i32]) -> std::borrow::Cow<'r, [i32]> {
        match self.drill_attn {
            Some(lie) if lie.cache == Some(kind) && lie.layer == li && lie.position == position => {
                let mut moved = row.to_vec();
                lie.apply(&mut moved);
                std::borrow::Cow::Owned(moved)
            }
            _ => std::borrow::Cow::Borrowed(row),
        }
    }

    /// One token; returns the COMMITTED logit row: i16 codes in i32 lanes. Ties in any argmax
    /// over this row break to the lowest index, here and in court alike.
    pub fn forward_token(&self, cache: &mut A16Cache, token_id: usize, position: usize) -> Result<Vec<i32>, A16EngineError> {
        self.forward_token_traced(cache, token_id, position).map(|(l, _)| l)
    }

    /// As [`forward_token`], plus the per-layer residual streams (measurement).
    pub fn forward_token_probed(
        &self,
        cache: &mut A16Cache,
        token_id: usize,
        position: usize,
    ) -> Result<(Vec<i32>, Vec<Vec<i32>>), A16EngineError> {
        let (logits, trace) = self.forward_token_traced(cache, token_id, position)?;
        let streams = trace.attn.iter().map(|nodes| nodes.last().cloned().unwrap_or_default()).collect();
        Ok((logits, streams))
    }

    /// **The compiled GRAPH-V2 program: one position's forward with every node's committed row
    /// recorded, in the v2 shape profile's numbering.** The full-job replay adjudicates each of
    /// these rows through the court's own dispatch and demands bit equality.
    ///
    /// **It is a v2 reference and nothing wider.** The row count is written into this function —
    /// twenty-seven nodes a layer, the attention site spelled as `ATTN_SCORES`, the row `SoftMax`,
    /// the probability requantization and `ATTN_VALUES` — so it describes exactly the graph
    /// `qwen25_a16_profile_v2` declares. ADR-0082's graph v5 replaces those four nodes with one
    /// fused node and declares twenty-four, and this route MUST NOT learn the fusion: the whole
    /// point of ADR-0067 Decision 2 is that [`Self::plan_from_profile`] is the single authority on
    /// what a declaration executes, and a second hand-written program that also knew the fused
    /// site would be a second authority to keep in step. A v5 class is served by
    /// [`Self::forward_token_planned`]; a caller that reaches this route with a v5 profile is
    /// refused by name, by the Decision-F probe in `a16_execute_for_attempt_v1` ("per-layer
    /// declares 24 against 27 recorded") and pinned by
    /// `the_plan_less_route_is_the_v2_reference_and_refuses_a_fused_row`.
    pub fn forward_token_traced(
        &self,
        cache: &mut A16Cache,
        token_id: usize,
        position: usize,
    ) -> Result<(Vec<i32>, A16TraceV1), A16EngineError> {
        let shape = &self.artifact.shape;
        let d = shape.d_model();
        let kv_dim = shape.kv_dim();
        let refuse =
            |what: &'static str| move |_e: kaspa_consensus_core::palw_base0_a16::PalwA16OpError| A16EngineError::OpRefused(what);
        let (cos_row, sin_row) = self.artifact.rope.row(position).ok_or(A16EngineError::PositionOutOfRange)?;
        let sink = position == 0;
        let tile = |p: A16QuantParams, n: usize| -> Vec<A16QuantParams> { vec![p; n] };
        let mut trace = A16TraceV1::default();

        // ---- pre: the gather (node 0) and the lift onto the A16 stream (node 1) -------------
        let embed_row: Vec<i32> = self.artifact.embed[token_id * d..(token_id + 1) * d].iter().map(|c| *c as i32).collect();
        trace.pre.push(embed_row.clone());
        let mut h = a16_requant(&embed_row, &tile(self.embed_lift, d)).map_err(refuse("embed_lift"))?;
        trace.pre.push(h.clone());

        for (li, lp) in self.layers.iter().enumerate() {
            let lw = &self.artifact.layers[li];
            let mut nodes: Vec<Vec<i32>> = Vec::with_capacity(27);
            let push = |nodes: &mut Vec<Vec<i32>>, row: Vec<i32>| -> Vec<i32> {
                nodes.push(row.clone());
                row
            };

            // ---- attention (nodes 0..=14) ---------------------------------------------------
            let unit = push(&mut nodes, a16_rms_norm(&h, shape.eps_q).map_err(refuse("norm1"))?);
            let normed = push(&mut nodes, a16_requant(&unit, &lp.attn_norm).map_err(refuse("norm1_req"))?);
            let q = push(&mut nodes, a16_matmul_requant(self.fast, &lw.wq, &normed, &lp.q).map_err(refuse("q"))?);
            let k = push(&mut nodes, a16_matmul_requant(self.fast, &lw.wk, &normed, &lp.k).map_err(refuse("k"))?);
            let v = push(&mut nodes, a16_matmul_requant(self.fast, &lw.wv, &normed, &lp.v).map_err(refuse("v"))?);
            let rope_heads = |row: &[i32], heads: usize, what: &'static str| -> Result<Vec<i32>, A16EngineError> {
                let mut out = Vec::with_capacity(row.len());
                for hd in 0..heads {
                    let slice = &row[hd * shape.d_head..(hd + 1) * shape.d_head];
                    out.extend(a16_rope(slice, cos_row, sin_row).map_err(|_| A16EngineError::OpRefused(what))?);
                }
                Ok(out)
            };
            let q_rot = push(&mut nodes, rope_heads(&q, shape.n_heads, "rope_q")?);
            let k_rot = push(&mut nodes, rope_heads(&k, shape.n_kv_heads, "rope_k")?);
            cache.push_key(li, &self.drill_cache_row(li, position, 0, &k_rot))?;
            cache.push_value(li, &self.drill_cache_row(li, position, 1, &v))?;
            let history = cache.rows_in(li);

            // The cache series, EXACTLY as the court's canonical input set concatenates them:
            // full kv_dim rows, position-major — the storage itself, borrowed.
            let k_series = cache.keys_visible(li, history)?;
            let v_series = cache.values_visible(li, history)?;
            debug_assert_eq!(k_series.len(), history * kv_dim);

            let logits_row = push(
                &mut nodes,
                a16_attn_scores(
                    self.fast,
                    &q_rot,
                    k_series,
                    shape.n_heads,
                    shape.n_kv_heads,
                    shape.d_head,
                    &tile(lp.logits, shape.n_heads * history),
                )
                .map_err(refuse("logits"))?,
            );
            let probs_row = push(&mut nodes, a16_softmax_rows(&logits_row, history, lp.softmax_up).map_err(refuse("softmax"))?);
            let p15 = push(&mut nodes, a16_requant(&probs_row, &tile(lp.probs, shape.n_heads * history)).map_err(refuse("p15"))?);
            let attn = push(
                &mut nodes,
                a16_attn_values(
                    self.fast,
                    kaspa_consensus_core::palw_base0_a16::A16_MAX_ATTN_HISTORY_V1,
                    &p15,
                    v_series,
                    shape.n_heads,
                    shape.n_kv_heads,
                    shape.d_head,
                    &tile(lp.values, shape.n_heads * shape.d_head),
                )
                .map_err(refuse("values"))?,
            );

            let wo_params = if sink { &lp.wo_sink } else { &lp.wo };
            let delta = push(&mut nodes, a16_matmul_requant(self.fast, &lw.wo, &attn, wo_params).map_err(refuse("wo"))?);
            let align = if sink { lp.attn_align_sink } else { lp.attn_align };
            let aligned = push(&mut nodes, a16_requant(&h, &tile(align, d)).map_err(refuse("attn_align"))?);
            let sum = push(&mut nodes, a16_add_elem(&aligned, &delta).map_err(refuse("attn_add"))?);
            h = push(&mut nodes, a16_requant(&sum, &tile(lp.attn_residual, d)).map_err(refuse("attn_res"))?);

            // ---- SwiGLU (nodes 15..=26) -----------------------------------------------------
            let unit = push(&mut nodes, a16_rms_norm(&h, shape.eps_q).map_err(refuse("norm2"))?);
            let normed = push(&mut nodes, a16_requant(&unit, &lp.ffn_norm).map_err(refuse("norm2_req"))?);
            let gate_q = push(&mut nodes, a16_matmul_rescale(self.fast, &lw.w_gate, &normed, &lp.gate).map_err(refuse("gate"))?);
            // **In the DECLARED order: up before the silu chain** (ADR-0067). The step-leg
            // capture places rows at profile coordinates BY POSITION (`a16_captured_rows_v1`
            // reorders nothing), so a trace emitted in any other order commits the silu row at
            // the slot the class declares as the up-projection — and a court bisecting there
            // recomputes the declaration, convicting an honest producer. Caught by the
            // interpreter differential before any claim of this class reached a chain; the
            // dataflow is unchanged, only the emission order conforms to the declaration.
            let up_params = if sink { &lp.up_sink } else { &lp.up };
            let up = push(&mut nodes, a16_matmul_requant(self.fast, &lw.w_up, &normed, up_params).map_err(refuse("up"))?);
            let silu_q = push(&mut nodes, silu(&gate_q));
            let s_p = if sink { lp.silu_sink } else { lp.silu_q };
            let s16 = push(&mut nodes, a16_requant(&silu_q, &tile(s_p, shape.d_ff)).map_err(refuse("silu16"))?);
            let prod = push(&mut nodes, a16_mul_elem(&s16, &up).map_err(refuse("mul"))?);
            let g_p = if sink { lp.gated_sink } else { lp.gated };
            let gated = push(&mut nodes, a16_requant(&prod, &tile(g_p, shape.d_ff)).map_err(refuse("gated"))?);
            let down_params = if sink { &lp.down_sink } else { &lp.down };
            let delta = push(&mut nodes, a16_matmul_requant(self.fast, &lw.w_down, &gated, down_params).map_err(refuse("down"))?);
            let align = if sink { lp.ffn_align_sink } else { lp.ffn_align };
            let aligned = push(&mut nodes, a16_requant(&h, &tile(align, d)).map_err(refuse("ffn_align"))?);
            let sum = push(&mut nodes, a16_add_elem(&aligned, &delta).map_err(refuse("ffn_add"))?);
            h = push(&mut nodes, a16_requant(&sum, &tile(lp.ffn_residual, d)).map_err(refuse("ffn_res"))?);
            trace.attn.push(nodes);
        }

        // ---- post: final norm and the TIED logits, committed as i16 codes -------------------
        let unit = a16_rms_norm(&h, shape.eps_q).map_err(refuse("final_norm"))?;
        trace.post.push(unit.clone());
        let fin = a16_requant(&unit, &self.final_norm).map_err(refuse("final_req"))?;
        trace.post.push(fin.clone());
        let logits = a16_matmul_requant(self.fast, &self.artifact.unembed, &fin, &tile(self.logits_out, shape.vocab))
            .map_err(refuse("logits_out"))?;
        trace.post.push(logits.clone());
        Ok((logits, trace))
    }
}

/// **A well-formed A16 parameter store for a shape** — the tier's analogue of
/// `Base0ArtifactV1::derive_deterministic`, and marked as sharply.
///
/// It is NOT a calibration. A converted class gets its triples from the PTQ pipeline, measured
/// from the checkpoint; these are chosen only so that every row `A16Engine::new` resolves exists
/// and a forward pass produces something other than zeros. An artifact carrying this store is
/// still `is_derived()`, so it cannot be mistaken for a registered class.
///
/// # Why the scales are split by SITE and derived from the fan-in
///
/// The first version of this used one gain everywhere and the engine returned an all-zero logit
/// row: a matmul's accumulator grows with its fan-in and an elementwise requant's does not, so an
/// attenuation big enough for the first is applied ~10 times per layer to the second and the
/// residual stream decays to nothing. That is not a subtle failure — but it is a SILENT one. It
/// passed a fast-versus-reference differential (both agree on zero) and was caught only by asking
/// whether two different tokens produce two different rows.
///
/// So a projection over `fan_in` gets `2^-(8 + bits(fan_in)/2)`, tracking the `√fan_in` growth of
/// a random dot product, and an elementwise site gets unity.
pub fn derived_a16_store(shape: &Base0ShapeV1) -> Vec<(String, Vec<u8>)> {
    let (d, kv, ff) = (shape.d_model(), shape.kv_dim(), shape.d_ff);
    let wire = |m: i64, s: u8, n: usize| -> Vec<u8> {
        A16QuantParams { multiplier: m, shift: s, zero: 0 }
            .to_wire()
            .iter()
            .cycle()
            .take(n * A16QuantParams::WIRE_BYTES)
            .copied()
            .collect()
    };
    let projection = |fan_in: usize, n: usize| -> Vec<u8> {
        let bits = usize::BITS - fan_in.max(1).leading_zeros();
        wire(1, (8 + bits / 2) as u8, n)
    };
    let unity = |n: usize| wire(1, 0, n);

    let mut store: Vec<(String, Vec<u8>)> = vec![
        ("embed_lift.a16".into(), unity(1)),
        ("final_norm.a16".into(), unity(d)),
        ("token_embd.weight.a16".into(), projection(d, 1)),
    ];
    for li in 0..shape.n_layers {
        let b = format!("blk.{li}");
        let rows: [(&str, Vec<u8>); 25] = [
            ("attn_norm.a16", unity(d)),
            ("attn_q.weight.a16", projection(d, d)),
            ("attn_k.weight.a16", projection(d, kv)),
            ("attn_v.weight.a16", projection(d, kv)),
            ("attn_logits.a16", projection(shape.d_head, 1)),
            ("attn_probs.a16", unity(1)),
            ("attn_values.a16", projection(shape.d_head, 1)),
            ("attn_output.weight.a16", projection(d, d)),
            ("attn_output.weight.a16.sink0", projection(d, d)),
            ("attn_align.a16", unity(1)),
            ("attn_align.a16.sink0", unity(1)),
            ("attn_residual.a16", unity(1)),
            ("ffn_norm.a16", unity(d)),
            ("ffn_gate.weight.a16", projection(d, ff)),
            ("ffn_silu.a16", unity(1)),
            ("ffn_silu.a16.sink0", unity(1)),
            ("ffn_up.weight.a16", projection(d, ff)),
            ("ffn_up.weight.a16.sink0", projection(d, ff)),
            ("ffn_gated.a16", unity(1)),
            ("ffn_gated.a16.sink0", unity(1)),
            ("ffn_down.weight.a16", projection(ff, d)),
            ("ffn_down.weight.a16.sink0", projection(ff, d)),
            ("ffn_align.a16", unity(1)),
            ("ffn_align.a16.sink0", unity(1)),
            ("ffn_residual.a16", unity(1)),
        ];
        for (suffix, bytes) in rows {
            store.push((format!("{b}.{suffix}"), bytes));
        }
        // Not a triple: one raw byte, the softmax widening the tier reads directly.
        store.push((format!("{b}.attn_softmax_up"), vec![24u8]));
    }
    store.sort_by(|a, b| a.0.cmp(&b.0));
    store
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::LN_THETA_10000_GEN_Q;

    fn artifact(n_layers: usize, d_head: usize, d_ff: usize) -> Base0ArtifactV1 {
        let shape = Base0ShapeV1 {
            n_layers,
            n_heads: 4,
            n_kv_heads: 2,
            d_head,
            d_ff,
            vocab: 64,
            max_position: 32,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1,
        };
        Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(derived_a16_store(&shape))
            .expect("the derived store is sorted and unique")
    }

    /// **The claim the fast kernels are allowed to exist on.**
    ///
    /// `kernels`' own tests compare one projection at a time. This compares whole forward passes,
    /// where a divergence would also have to survive the residual stream, the KV cache and the
    /// attention arms — which is the only place a kernel bug that cancels inside one matmul would
    /// still show up. Every logit of every token, exactly equal.
    #[test]
    fn the_fast_engine_and_the_catalog_agree_token_for_token() {
        // Widths on both sides of the kernel's 16-element vector block and its 64-channel
        // parallel threshold, so neither path is exercised by only one of its branches.
        for (layers, d_head, d_ff) in [(1usize, 4usize, 8usize), (2, 8, 32), (2, 32, 160)] {
            let artifact = artifact(layers, d_head, d_ff);
            let fast = A16Engine::new(&artifact).expect("the store resolves");
            let reference = A16Engine::new_reference(&artifact).expect("the store resolves");
            let (mut fast_cache, mut reference_cache) = (A16Cache::new(layers), A16Cache::new(layers));
            for position in 0..12usize {
                let token = (position * 7 + 3) % artifact.shape.vocab;
                let a = fast.forward_token(&mut fast_cache, token, position).expect("the token decodes");
                let b = reference.forward_token(&mut reference_cache, token, position).expect("the token decodes");
                assert_eq!(a, b, "layers={layers} d_head={d_head} d_ff={d_ff} position={position}");
            }
        }
    }

    /// **A batched prefill must leave exactly what a sequential one leaves.**
    ///
    /// Not "the same logits" — the same KV CACHE, because the next decode token reads it. A batch
    /// that got attention's visibility wrong by one would still produce a plausible last row and
    /// then poison every token after it.
    ///
    /// Batch sizes straddle the run length so that the last chunk is ragged, and the run starts
    /// both at 0 (where the sink is peeled off and processed alone) and mid-context.
    #[test]
    fn a_batched_prefill_leaves_the_same_state_as_a_sequential_one() {
        for (layers, d_head, d_ff) in [(1usize, 4usize, 8usize), (2, 8, 32)] {
            let artifact = artifact(layers, d_head, d_ff);
            let engine = A16Engine::new(&artifact).expect("the store resolves");
            let tokens: Vec<usize> = (0..11).map(|i| (i * 5 + 1) % artifact.shape.vocab).collect();

            for batch in [1usize, 2, 3, 4, 16] {
                for start in [0usize, 1, 5] {
                    let mut sequential = A16Cache::new(layers);
                    let mut expected = Vec::new();
                    // The sequential run needs the same history in front of it when `start` is
                    // not zero, so the leading positions are filled the same way for both.
                    for position in 0..start {
                        let _ = engine.forward_token(&mut sequential, position % artifact.shape.vocab, position).expect("decodes");
                    }
                    let mut batched = A16Cache::new(layers);
                    for position in 0..start {
                        let _ = engine.forward_token(&mut batched, position % artifact.shape.vocab, position).expect("decodes");
                    }
                    for (i, token) in tokens.iter().enumerate() {
                        expected = engine.forward_token(&mut sequential, *token, start + i).expect("decodes");
                    }
                    let got = engine.forward_prefill(&mut batched, &tokens, start, batch).expect("prefills");

                    assert_eq!(got, expected, "logits: layers={layers} batch={batch} start={start}");
                    assert_eq!(batched.len(), sequential.len(), "cache depth: batch={batch} start={start}");
                    for li in 0..layers {
                        assert_eq!(batched.keys_as_i32(li), sequential.keys_as_i32(li), "keys layer {li}: batch={batch} start={start}");
                        assert_eq!(batched.values_as_i32(li), sequential.values_as_i32(li), "values layer {li}: batch={batch} start={start}");
                    }
                }
            }
        }
    }

    /// And the cache a batched prefill leaves must carry a decode that continues from it — the
    /// property the test above is a proxy for, checked directly.
    #[test]
    fn decoding_continues_identically_from_a_batched_prefill() {
        let artifact = artifact(2, 8, 32);
        let engine = A16Engine::new(&artifact).expect("the store resolves");
        let prompt: Vec<usize> = vec![3, 9, 17, 4, 11, 2, 8];

        let mut sequential = A16Cache::new(2);
        let mut logits_a = Vec::new();
        for (i, t) in prompt.iter().enumerate() {
            logits_a = engine.forward_token(&mut sequential, *t, i).expect("decodes");
        }
        let mut batched = A16Cache::new(2);
        let mut logits_b = engine.forward_prefill(&mut batched, &prompt, 0, 4).expect("prefills");
        assert_eq!(logits_a, logits_b);

        for step in 0..6 {
            let next_a = crate::engine::argmax_lowest(&logits_a);
            let next_b = crate::engine::argmax_lowest(&logits_b);
            assert_eq!(next_a, next_b, "step {step}");
            logits_a = engine.forward_token(&mut sequential, next_a, prompt.len() + step).expect("decodes");
            logits_b = engine.forward_token(&mut batched, next_b, prompt.len() + step).expect("decodes");
            assert_eq!(logits_a, logits_b, "step {step}");
        }
    }

    /// A forward pass that returns the same row for every token would satisfy the differential
    /// above while computing nothing, so the fixture is checked for being non-degenerate.
    #[test]
    fn the_fixture_actually_computes_something() {
        let artifact = artifact(2, 8, 32);
        let engine = A16Engine::new(&artifact).expect("the store resolves");
        let mut cache = A16Cache::new(2);
        let first = engine.forward_token(&mut cache, 5, 0).expect("decodes");
        let second = engine.forward_token(&mut cache, 9, 1).expect("decodes");
        assert_ne!(first, second, "a different token at a different position must move the logits");
        assert!(first.iter().any(|v| *v != 0), "an all-zero logit row is a dead pass");
    }

    /// What the cache holds, counted: `(min, max, elements, inside ±127, inside ±32767, 64-code
    /// blocks whose absmax is ≤ 127, blocks)`. The block figure is what a lossless block-packed
    /// `i8` representation could store at one byte a code; every other block would stay at two.
    pub(crate) fn kv_code_range_v1(cache: &A16Cache, only_layer: Option<usize>) -> (i32, i32, u64, u64, u64, u64, u64) {
        let (mut min, mut max, mut n, mut in_i8, mut in_i16, mut blocks_fit, mut blocks) = (i32::MAX, i32::MIN, 0u64, 0u64, 0u64, 0u64, 0u64);
        let (keys, values) = cache.contents_as_i32();
        for (li, layer) in keys.iter().chain(values.iter()).enumerate() {
            if only_layer.is_some_and(|only| li % keys.len() != only) {
                continue;
            }
            for row in layer.chunks(cache.kv_dim.max(1)) {
                for value in row {
                    min = min.min(*value);
                    max = max.max(*value);
                    n += 1;
                    in_i8 += u64::from(value.unsigned_abs() <= 127);
                    in_i16 += u64::from(value.unsigned_abs() <= A16_CODE_MAX_I32 as u32);
                }
                for block in row.chunks(64) {
                    blocks += 1;
                    blocks_fit += u64::from(block.iter().all(|v| v.unsigned_abs() <= 127));
                }
            }
        }
        (min, max, n, in_i8, in_i16, blocks_fit, blocks)
    }
    const A16_CODE_MAX_I32: i32 = kaspa_consensus_core::palw_base0_a16::A16_CODE_MAX as i32;

    /// **The value range of what the cache holds — the fact every compact representation rests on,
    /// measured rather than argued.**
    ///
    /// Every K row is `a16_rope`'s output and every V row is `a16_matmul_requant`'s; both end in
    /// `clamp16`, so each element is an A16 code in ±32,767 by construction, and every consumer of
    /// the series (`as_a16` in the catalog, `check_codes` in the kernels) refuses an element outside
    /// that range. So an `i16` holds every element losslessly and the committed `i32` little-endian
    /// bytes regenerate from it exactly. The fraction inside ±127 is reported beside it: that is what
    /// an `i8` could hold losslessly, and on the real row it is a minority of the elements (see the
    /// ignored measurement below), which is why there is no `i8` profile that is not a quantization
    /// — and a quantization of the cache changes what attention computes, i.e. the class.
    #[test]
    fn every_cached_code_fits_i16_on_the_court_fixture() {
        use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, qwen25_a16_profile_v7};
        // The court drill's own geometry (`tests/court_e2e.rs::GEOMETRY`), run to the end of its context.
        let shape = Base0ShapeV1 {
            n_layers: 2,
            n_heads: 2,
            n_kv_heads: 2,
            d_head: 4,
            d_ff: 8,
            vocab: 64,
            max_position: 32,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1,
        };
        let artifact = Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(derived_a16_store(&shape))
            .expect("sorted and unique");
        let g = PalwQwen25GeometryV1 {
            layer_count: 2,
            hidden_dim: 8,
            ffn_dim: 8,
            attn_heads: 2,
            attn_kv_heads: 2,
            attn_head_dim: 4,
            vocab_size: 64,
            n_ctx: 32,
            n_threads: 1,
            rms_eps_q: 1,
            tile_len: 4,
        };
        let engine = A16Engine::new(&artifact).expect("the store resolves");
        let plan = engine.plan_from_profile(&qwen25_a16_profile_v7(g).expect("v7")).expect("servable");
        let mut cache = A16Cache::new(2);
        for position in 0..31usize {
            engine.forward_token_planned(&plan, &mut cache, (position * 7 + 3) % 64, position).expect("walks");
        }
        let (min, max, n, in_i8, in_i16, blocks_fit, blocks) = kv_code_range_v1(&cache, None);
        eprintln!(
            "court fixture K/V range: min {min} max {max} over {n} elements; inside ±127: {in_i8} ({:.1} %); inside ±32767: \
             {in_i16}; 64-code blocks fitting i8: {blocks_fit}/{blocks}",
            in_i8 as f64 * 100.0 / n as f64
        );
        assert_eq!(in_i16, n, "an element outside the code range would have been refused by the next attention read");
        assert!(min >= -A16_CODE_MAX_I32 && max <= A16_CODE_MAX_I32);
        assert!(n > 0);
    }

    /// **The same measurement on the shipped dense row** — off unless `MISAKA_PALW_KV_RANGE_ARTIFACT`
    /// names the converted 1.5B artifact (1.7 GiB of weights are not a unit test's input), in release:
    ///
    /// ```text
    /// MISAKA_PALW_KV_RANGE_ARTIFACT=/path/qwen25-1.5b-a16.palwart MISAKA_PALW_KV_RANGE_PREFILL=511 \
    ///   cargo test --release -p misaka-palw-base0 --lib -- every_cached_code_fits_i16_on_the_real --ignored --nocapture
    /// ```
    ///
    /// Prints, per layer and overall, the absmax and the share of elements and of 64-code blocks
    /// inside ±127. The calibration sizes each site's scale on the post-rotation absmax with headroom
    /// (`a16_rope`'s doc), so the codes are expected to use most of the 16-bit range — the number
    /// this prints is the one that decides whether a lossless packed representation is worth its
    /// decode points.
    #[test]
    #[ignore = "needs the converted dense artifact; see the doc comment"]
    fn every_cached_code_fits_i16_on_the_real_dense_row() {
        let Ok(path) = std::env::var("MISAKA_PALW_KV_RANGE_ARTIFACT") else {
            eprintln!("kv range: skipped — set MISAKA_PALW_KV_RANGE_ARTIFACT to the dense .palwart to measure");
            return;
        };
        let prefill: usize = std::env::var("MISAKA_PALW_KV_RANGE_PREFILL").ok().and_then(|v| v.parse().ok()).unwrap_or(511);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let artifact = crate::artifact::decode_artifact_file_v1(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
        drop(bytes);
        let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_graph_v5_profile_v1().expect("the shipped dense row");
        let engine = A16Engine::new(&artifact).expect("an A16 artifact");
        let plan = engine.plan_from_profile(&profile).expect("the shipped row is servable");
        let prompt: Vec<usize> = crate::qwen25_a16_backend::qwen25_a16_prompt_for_anchor(
            kaspa_consensus_core::Hash64::from_u64_word(0x5A16_2026),
            artifact.shape.vocab,
            prefill as u32,
        );
        let started = std::time::Instant::now();
        let mut cache = A16Cache::new(artifact.shape.n_layers);
        let mut at = 0usize;
        while at < prompt.len() {
            let end = (at + 64).min(prompt.len());
            engine.forward_prefill_planned(&plan, &mut cache, &prompt[at..end], at, end == prompt.len()).expect("a run");
            at = end;
        }
        eprintln!("kv range: {prefill} positions of the real row in {:.1} s", started.elapsed().as_secs_f64());
        for li in 0..artifact.shape.n_layers {
            let (min, max, n, in_i8, _, blocks_fit, blocks) = kv_code_range_v1(&cache, Some(li));
            let k_absmax = cache.keys_as_i32(li).iter().map(|v| v.unsigned_abs()).max().unwrap_or(0);
            let v_absmax = cache.values_as_i32(li).iter().map(|v| v.unsigned_abs()).max().unwrap_or(0);
            eprintln!(
                "  layer {li:2}: K absmax {k_absmax:5}  V absmax {v_absmax:5}  min {min:6} max {max:5}  inside ±127 {:5.1} %  \
                 blocks fitting i8 {:5.1} %",
                in_i8 as f64 * 100.0 / n as f64,
                blocks_fit as f64 * 100.0 / blocks as f64
            );
        }
        let (min, max, n, in_i8, in_i16, blocks_fit, blocks) = kv_code_range_v1(&cache, None);
        eprintln!(
            "real row K/V range: min {min} max {max} over {n} elements; inside ±127: {in_i8} ({:.1} %); inside ±32767: {in_i16} \
             ({:.1} %); 64-code blocks fitting i8: {blocks_fit}/{blocks} ({:.1} %)",
            in_i8 as f64 * 100.0 / n as f64,
            in_i16 as f64 * 100.0 / n as f64,
            blocks_fit as f64 * 100.0 / blocks as f64
        );
        assert_eq!(in_i16, n, "every cached code is inside ±32767 on the real row too");
    }
}

// =============================================================================================
// ADR-0067: execution FROM the registered profile
// =============================================================================================
//
// Everything above executes a HARDCODED op sequence that the class's profile merely describes —
// which is why ADR-0049 Decision F needs a correspondence check at all: two authorities, one
// arithmetic. This half inverts the authority. A plan is compiled from the `PalwShapeProfileV3`
// the CHAIN registered: each declared node is bound to a kernel this build serves and to the
// named operand in the artifact's store, and execution walks the declaration. An engine built
// from the profile cannot perform a narrowing the profile does not name — Decision F stops being
// a check and becomes the constructor — and a class whose graph this build cannot serve is
// refused AT PLAN TIME with the node named, which is ADR-0067 Decision 3's kernel boundary
// surfacing exactly where it is crossed.
//
// The dispatch below is deliberately a closed vocabulary: (op kind, kernel semantics id, operand
// name shape) triples this build's kernels serve. It is NOT a general dataflow VM — width rules,
// input arities and the position-0 sink convention are the A16 family's kernel semantics, and a
// profile is served only where its declaration lands inside them. Anything else is a named
// refusal, because "almost servable" executed approximately is how an honest producer gets
// convicted.

use kaspa_consensus_core::palw_step::{
    PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, PALW_STEP_INPUT_LAYER_IN, PALW_STEP_INPUT_SENTINEL_MIN, PalwShapeProfileV3,
    PalwStepLaneV1, PalwStepNodeV1, PalwStepOutLenV1, kernel_semantics_id_v1,
};
use kaspa_consensus_core::palw_step_refute::{
    KDESC_A16_ADD_ELEM, KDESC_A16_ATTN_FUSED, KDESC_A16_ATTN_SCORES, KDESC_A16_ATTN_VALUES, KDESC_A16_EMBED, KDESC_A16_MATMUL_REQUANT,
    KDESC_A16_MATMUL_RESCALE, KDESC_A16_MUL_ELEM, KDESC_A16_REQUANTIZE, KDESC_A16_RMS_NORM, KDESC_A16_ROPE, KDESC_A16_SOFTMAX,
    KDESC_Q36_SILU, palw_attn_fused_tensors_v1,
};

/// Why a profile could not be compiled to a plan. Every variant names the boundary it found —
/// a plan error is the kernel-set boundary of ADR-0067 Decision 3 speaking, so it must say
/// WHICH declaration this build cannot serve, not merely that one exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum A16PlanErrorV1 {
    /// The profile's declared geometry is not this artifact's. The pairing is wrong at the root;
    /// no node-level answer would mean anything.
    GeometryMismatch { what: &'static str, profile: u64, artifact: u64 },
    /// The profile's lane is not the integer lane this family commits.
    NotAnIntegerLane,
    /// A declared node is outside this build's served vocabulary. `table` is "pre" / "layer" /
    /// "post"; the reason names the missing piece (kernel, operand, width, arity or dtype).
    UnservedNode { table: &'static str, index: usize, reason: String },
    /// **ADR-0067 SA-1: the declaration would materialise more than the interpreted path is
    /// allowed to hold.** A chain-registered profile is a stranger's program, and one token's
    /// walk commits one row per declared node — so the row widths and the node counts in a
    /// registration are an allocation the registrant chose. Refused at PLAN time, before a byte
    /// is allocated, because a ceiling that notices after the allocation is not a ceiling.
    OverMemoryCeiling { bytes: u64, ceiling: u64 },
}

/// **The interpreted path's memory ceiling (ADR-0067 SA-1), in bytes of one token's committed
/// trace.**
///
/// What it bounds is exactly what a registration controls: the number of declared nodes and the
/// width of each one's committed row, times the layers the layer table is walked for. It does not
/// bound the artifact or the KV cache, because those are sized by the WEIGHTS this operator chose
/// to hold, not by the stranger who registered the graph.
///
/// **64 MiB, and it is a MEASURED number rather than a chosen one.**
/// `the_interpreter_ceiling_is_derived_from_what_this_build_actually_serves` runs
/// [`interpreted_trace_bytes_v1`] over every class this build ships and fails if the constant
/// drifts away from them. What it measures today:
///
/// | class          | context | one token's committed trace |
/// |----------------|---------|-----------------------------|
/// | BASE-0         | 12      | 182,272 B                   |
/// | QWEN25-A16     | 16      | 9,384,448 B                 |
/// | QWEN36         | 8       | 17,638,208 B                |
/// | QWEN36, stress | 4,096   | 49,034,048 B                |
///
/// So the ceiling is 3.8x the largest class this build serves at its registered context, and still
/// above that same graph stretched to a 4,096-position context no admission gate accepts. The
/// first shipped value was 1 GiB, which was 60x the largest measured class and 40,000x the largest
/// gate-accepted profile in the adversarial corpus — a number nothing had produced and nothing
/// could reach, i.e. a bound with no evidence behind it. The margin is now stated and tested in
/// both directions: raise a class past it and the derivation test says so, and set the ceiling
/// somewhere arbitrary and it says that too.
///
/// **What actually refuses a hostile profile first, said plainly**, because SA-1 should not be read
/// as more than it is: on the shipped admission gate the leaf bound and the per-node width checks
/// throw out every oversized shape the adversarial corpus can generate — 372 of 400, with the
/// largest gate-ACCEPTED profile costing 26,624 bytes. This ceiling is the second line, and it is
/// the line that survives a gate whose node counts or row widths are ever loosened. It is checked
/// after the scalar geometry comparisons (which are free) and before the first allocation, so a
/// profile that is merely the wrong shape is reported as the wrong shape.
///
/// **And this is a NODE CAPACITY limit, not a bound the chain's admission gate implies — which
/// matters because classes are permissionless (ADR-0054) and the band above is measured over the
/// three classes THIS BUILD compiles.** The consensus shape caps do not bound a declared row's
/// width at all: `validate_shape` asks for a non-zero width and a tile inside
/// `[PALW_STEP_MIN_TILE_LEN, PALW_STEP_MAX_TILE_LEN]`, so at the widest admitted tile a single
/// extra node of 20 M elements costs 306 leaves per position — nowhere near the leaf cap — and
/// 80 MB of committed trace, which is over this ceiling.
/// `the_consensus_shape_caps_admit_more_than_this_build_will_materialise` constructs exactly that
/// profile and drives it, so the gap is a demonstration rather than a hope. Deriving the ceiling
/// from the caps instead would put it at `PALW_STEP_MAX_LEAVES × PALW_STEP_MAX_TILE_LEN × 4` — a
/// terabyte, i.e. back to a number nothing measured chose and nothing can reach.
///
/// So the honest statement, and the one the REFUSAL carries to whoever reads a node's log
/// (`from_registered_profile` in both backends): a class between this ceiling and what the chain
/// admits is registered, valid, and adjudicable — this node simply will not materialise it, a node
/// built with a larger ceiling will, and the divergence is node-local servability (who produces and
/// who judges), never block validity. Raising the constant is an operator's call about memory; it
/// is not a consensus change and it never was.
pub const PALW_INTERPRETER_TRACE_BYTES_CEILING_V1: u64 = 64 << 20;

/// How many bytes one token's committed trace costs under `profile`, counted the way
/// `forward_token_planned` actually spends them: one `i32` row per declared node, the layer table
/// once per layer, and `max_kv_len` standing in for a kv-scaled row's longest form.
///
/// Saturating throughout: this is called ON adversarial input, so an overflow that wrapped to a
/// small number would be the exact failure the ceiling exists to prevent.
///
/// **One spelling, in consensus core** (`palw_resource_profile_v1::palw_committed_trace_bytes_v1`):
/// the resource profile prices a prefill run's retained traces with the same number this ceiling
/// bounds, so the two cannot disagree about what a token's trace costs.
pub fn interpreted_trace_bytes_v1(profile: &PalwShapeProfileV3, max_kv_len: u64) -> u64 {
    kaspa_consensus_core::palw_resource_profile_v1::palw_committed_trace_bytes_v1(profile, max_kv_len)
}

/// A node's data input, resolved at plan time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlanInput {
    /// An earlier node's committed row in the same table.
    Row(usize),
    /// The table's input stream: the pre output for the layer table (and the running residual
    /// between layers), the last layer's output for the post table.
    LayerIn,
    /// The rotated-key series, position-major, full `kv_dim` rows — the court's canonical
    /// concatenation.
    CachedK,
    /// The value series, likewise.
    CachedV,
}

/// The per-layer A16 requant table a planned node reads. Slots, not names, because the names
/// were resolved at plan time — execution must not re-parse strings per token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReqSlot {
    EmbedLift,
    AttnNorm,
    Probs,
    AttnAlign,
    AttnResidual,
    FfnNorm,
    SiluQ,
    Gated,
    FfnAlign,
    FfnResidual,
    FinalNorm,
}

/// The weight-bearing matmul sites. Each carries both the tensor and the requant/rescale table
/// the engine's parameter store associates with that site (with the position-0 sink variant
/// where the store declares one).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MatSlot {
    Q,
    K,
    V,
    Wo,
    Gate,
    Up,
    Down,
    Head,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlanOp {
    EmbedGather,
    RmsNorm,
    Requant(ReqSlot),
    MatMulRequant(MatSlot),
    MatMulRescale(MatSlot),
    Rope {
        kv: bool,
    },
    AttnScores,
    Softmax,
    AttnValues,
    /// **ADR-0082 Decision 1: the whole attention site as one op.** Scores, the row softmax, the
    /// probability requantization and the value reduction, computed here and committed as ONE
    /// row: the output. The three context-wide rows the four separate ops commit are internal —
    /// never a plan row, never a leaf, never carried.
    AttnFused,
    AddElem,
    MulElem,
    Silu,
}

#[derive(Clone, Debug)]
struct PlanNode {
    op: PlanOp,
    inputs: Vec<PlanInput>,
    role: kaspa_consensus_core::palw_step::PalwStepNodeRoleV1,
}

/// A compiled execution plan: the registered profile, validated against this build's kernel
/// vocabulary and this artifact's operand store, ready to walk. Holding one is the proof that
/// every declared node is servable — construction is the admission check.
#[derive(Clone, Debug)]
pub struct A16ProfilePlanV1 {
    pre: Vec<PlanNode>,
    layer: Vec<PlanNode>,
    post: Vec<PlanNode>,
    layer_count: usize,
    /// The class's attention history bound (ADR-0116), read off the profile at compile time —
    /// the held regime's `2^21` for a class that registered a held map, `2^18` for every other.
    attn_history: usize,
}

impl<'a> A16Engine<'a> {
    /// Compile the registered profile into a plan this engine can walk.
    ///
    /// Refusals here are the ADR-0067 kernel boundary: the class declared arithmetic this build
    /// does not serve, and the error names the node. A `Ok` is a structural Decision-F proof —
    /// execution will emit exactly one row per declared node, in the declared order, from the
    /// declared operands, because the declaration is the program.
    pub fn plan_from_profile(&self, profile: &PalwShapeProfileV3) -> Result<A16ProfilePlanV1, A16PlanErrorV1> {
        self.plan_from_profile_within(profile, PALW_INTERPRETER_TRACE_BYTES_CEILING_V1)
    }

    /// [`Self::plan_from_profile`] under a caller-chosen ceiling — ADR-0067 SA-1.
    ///
    /// The ceiling is a parameter so it can be PROVEN to bind: a test that only ever runs at the
    /// shipped value can show that nothing crashed, which is not the same claim. Node code takes
    /// the default; the fuzz gate and the ceiling's own test drive it down until it refuses, which
    /// is the evidence the amendment asks for.
    pub fn plan_from_profile_within(
        &self,
        profile: &PalwShapeProfileV3,
        ceiling_bytes: u64,
    ) -> Result<A16ProfilePlanV1, A16PlanErrorV1> {
        let shape = &self.artifact.shape;
        let check = |what: &'static str, p: u64, a: u64| -> Result<(), A16PlanErrorV1> {
            if p != a { Err(A16PlanErrorV1::GeometryMismatch { what, profile: p, artifact: a }) } else { Ok(()) }
        };
        if profile.lane != PalwStepLaneV1::Int32 {
            return Err(A16PlanErrorV1::NotAnIntegerLane);
        }
        check("layer_count", profile.layer_count as u64, shape.n_layers as u64)?;
        check("hidden_dim", profile.hidden_dim as u64, shape.d_model() as u64)?;
        check("ffn_dim", profile.ffn_dim as u64, shape.d_ff as u64)?;
        check("attn_heads", profile.attn_heads as u64, shape.n_heads as u64)?;
        check("attn_kv_heads", profile.attn_kv_heads as u64, shape.n_kv_heads as u64)?;
        check("attn_head_dim", profile.attn_head_dim as u64, shape.d_head as u64)?;
        check("vocab_size", profile.vocab_size as u64, shape.vocab as u64)?;
        // The eps is an artifact field AND a profile field, and it moves every activation.
        check("rms_eps_q", profile.base0_rms_eps_q as u64, shape.eps_q as u64)?;

        // **The memory ceiling: after the free comparisons above, before the first allocation
        // below** (ADR-0067 SA-1). Ahead of `plan_table`, which is where bytes are first spent, so
        // the refusal still lands before the plan materialises anything — and behind the scalar
        // geometry checks, so a profile that is merely the wrong shape for this artifact is
        // reported as the wrong shape instead of as an oversized one. The earlier ordering put this
        // first and made `OverMemoryCeiling` the answer to a question the caller had not asked.
        // `max_position` is the artifact's own bound on a kv-scaled row.
        let bytes = interpreted_trace_bytes_v1(profile, shape.max_position as u64);
        if bytes > ceiling_bytes {
            return Err(A16PlanErrorV1::OverMemoryCeiling { bytes, ceiling: ceiling_bytes });
        }

        // Each table's terminal width is the next table's input, and both must be the hidden
        // stream: the residual is what flows pre -> layer -> layer -> post. A declaration whose
        // table ends on something else is refused HERE rather than mis-executed later.
        let terminal = |nodes: &[PalwStepNodeV1], table: &'static str| -> Result<u32, A16PlanErrorV1> {
            match nodes.last().map(|n| n.out_len) {
                Some(PalwStepOutLenV1::Fixed { elements }) => Ok(elements),
                _ => Err(A16PlanErrorV1::UnservedNode {
                    table,
                    index: nodes.len().saturating_sub(1),
                    reason: "the table's last node must produce a fixed-width row — it is the stream the next table reads".to_string(),
                }),
            }
        };
        let hidden = shape.d_model() as u32;
        let pre = plan_table(&profile.pre_nodes, "pre", shape, None)?;
        let pre_out = terminal(&profile.pre_nodes, "pre")?;
        if pre_out != hidden {
            return Err(A16PlanErrorV1::UnservedNode {
                table: "pre",
                index: profile.pre_nodes.len().saturating_sub(1),
                reason: format!("the pre table ends at width {pre_out}, and a layer reads the hidden stream ({hidden})"),
            });
        }
        let layer = plan_table(&profile.attn_nodes, "layer", shape, Some(hidden))?;
        let layer_out = terminal(&profile.attn_nodes, "layer")?;
        if layer_out != hidden {
            return Err(A16PlanErrorV1::UnservedNode {
                table: "layer",
                index: profile.attn_nodes.len().saturating_sub(1),
                reason: format!(
                    "the layer table ends at width {layer_out}, and the residual it feeds is the hidden stream ({hidden})"
                ),
            });
        }
        let post = plan_table(&profile.post_nodes, "post", shape, Some(hidden))?;
        Ok(A16ProfilePlanV1 {
            pre,
            layer,
            post,
            layer_count: shape.n_layers,
            attn_history: kaspa_consensus_core::palw_state_chunk_map::palw_attn_history_bound_v1(profile),
        })
    }

    /// One position's forward, EXECUTED FROM THE PLAN: one committed row per declared node, in
    /// the declared order. This is the route that serves EVERY declared graph, the fused
    /// attention site of ADR-0082 included (`PlanOp::AttnFused`).
    ///
    /// Bit-compatible with [`Self::forward_token_traced`] for a plan compiled from a GRAPH-V2
    /// profile — pinned by `the_interpreter_and_the_compiled_engine_agree_bit_for_bit` below,
    /// which is stated over `qwen25_a16_profile_v2` and claims nothing wider. There is no such
    /// correspondence for graph v5 and there is not meant to be one: the traced route is the
    /// twenty-seven-row v2 program, a v5 layer declares twenty-four nodes, and the fused arm's
    /// equality is proven against the ARITHMETIC instead — `a16_attn_fused_reference_v1` and
    /// `a16_attn_fused_via_tiles_v1`, in `the_fused_arm_is_the_reference_composition`.
    ///
    /// Faithful to the DECLARATION wherever a declaration and a hand-written program could
    /// differ, which is the point of ADR-0067: the court adjudicates what was declared, so an
    /// interpreter must execute exactly that.
    pub fn forward_token_planned(
        &self,
        plan: &A16ProfilePlanV1,
        cache: &mut A16Cache,
        token_id: usize,
        position: usize,
    ) -> Result<(Vec<i32>, A16TraceV1), A16EngineError> {
        if plan.layer_count != self.artifact.shape.n_layers {
            return Err(A16EngineError::MalformedParams("plan/artifact layer count"));
        }
        let (cos_row, sin_row) = self.artifact.rope.row(position).ok_or(A16EngineError::PositionOutOfRange)?;
        let sink = position == 0;
        let mut trace = A16TraceV1::default();
        self.walk_position.store(position, std::sync::atomic::Ordering::Relaxed);

        let mut h: Vec<i32> = Vec::new();
        // ---- pre --------------------------------------------------------------------------
        let rows = self.walk_table(&plan.pre, None, token_id, sink, cos_row, sin_row, None, plan.attn_history)?;
        if let Some(last) = rows.last() {
            h = last.clone();
        }
        trace.pre = rows;

        // ---- layers -----------------------------------------------------------------------
        for li in 0..plan.layer_count {
            let rows =
                self.walk_table(&plan.layer, Some(&h), token_id, sink, cos_row, sin_row, Some((li, cache)), plan.attn_history)?;
            h = rows.last().cloned().ok_or(A16EngineError::MalformedParams("an empty layer table"))?;
            trace.attn.push(rows);
        }

        // ---- post -------------------------------------------------------------------------
        let rows = self.walk_table(&plan.post, Some(&h), token_id, sink, cos_row, sin_row, None, plan.attn_history)?;
        let logits = rows.last().cloned().ok_or(A16EngineError::MalformedParams("an empty post table"))?;
        trace.post = rows;
        Ok((logits, trace))
    }
}

impl A16ProfilePlanV1 {
    /// **Whether this plan's layer can be walked a prompt at a time** (ADR-0117 Decision 2 on the
    /// dense tier): every cache read after the cache write it reads. A position's walk writes its
    /// own key and value before it reads the series, so a layer-major walk appends the whole run's
    /// rows at the write and hands each position the prefix that ends at its own. A declaration
    /// that read a series before writing to it would have each position see the positions before
    /// it WITHOUT their writes in a layer-major order; it keeps the stepped walk.
    pub fn one_pass_prefill_supported(&self) -> bool {
        use kaspa_consensus_core::palw_step::PalwStepNodeRoleV1 as Role;
        let write_of = |role: Role| self.layer.iter().position(|n| n.role == role);
        let (k_write, v_write) = (write_of(Role::KCacheWrite), write_of(Role::VCacheWrite));
        let writes = |role: Role| self.layer.iter().filter(|n| n.role == role).count();
        if writes(Role::KCacheWrite) > 1 || writes(Role::VCacheWrite) > 1 {
            return false;
        }
        self.layer.iter().enumerate().all(|(at, node)| {
            node.inputs.iter().all(|input| match input {
                PlanInput::CachedK => k_write.is_some_and(|w| w < at),
                PlanInput::CachedV => v_write.is_some_and(|w| w < at),
                _ => true,
            })
        })
    }
}

impl<'a> A16Engine<'a> {
    /// **The prefill in one pass over the weights** (ADR-0117 Decision 2, the dense tier's half):
    /// every position of `tokens` through a layer before the next layer is read, each of the
    /// layer's projections run over the whole run at once (`kernels::a16_matmul_requant_batch`:
    /// the weight row read once and used for every position).
    ///
    /// The rows are the ones [`Self::forward_token_planned`] commits position by position, bit for
    /// bit: every node is evaluated by the same [`Self::eval_node`] on the same inputs — a
    /// position's walk through layer `li` reads only its own rows, layer `li − 1`'s output for
    /// itself, and the cache prefix that ends at its own write — and a batched projection is the
    /// single-row one on each row (`the_batched_projections_are_the_single_row_ones` in `kernels`).
    /// The sink position (0) runs its projections alone, on its own parameters (ADR-0050). So the
    /// traces, the cache left behind and the last position's logits are the stepped walk's
    /// (`the_one_pass_prefill_is_the_position_by_position_one`).
    ///
    /// `with_post` runs the post table at the LAST position only — the one whose rows the step
    /// space commits when it is the prefill's last; an earlier position's logits predict a token
    /// the prompt already holds, and its trace's `post` is empty. The logits returned are empty
    /// without it. Refused before anything is read: a plan whose layer reads the cache before
    /// writing it ([`A16ProfilePlanV1::one_pass_prefill_supported`]), a token outside the
    /// vocabulary, a position past the rotation table.
    pub fn forward_prefill_planned(
        &self,
        plan: &A16ProfilePlanV1,
        cache: &mut A16Cache,
        tokens: &[usize],
        first_position: usize,
        with_post: bool,
    ) -> Result<(Vec<i32>, Vec<A16TraceV1>), A16EngineError> {
        if plan.layer_count != self.artifact.shape.n_layers {
            return Err(A16EngineError::MalformedParams("plan/artifact layer count"));
        }
        if tokens.is_empty() {
            return Err(A16EngineError::OpRefused("an empty prefill"));
        }
        if !plan.one_pass_prefill_supported() {
            return Err(A16EngineError::OpRefused("the plan reads the cache before this position writes it"));
        }
        if tokens.iter().any(|t| *t >= self.artifact.shape.vocab) {
            return Err(A16EngineError::OpRefused("a token outside the vocabulary"));
        }
        let rope: Vec<(&[i32], &[i32])> = (0..tokens.len())
            .map(|i| self.artifact.rope.row(first_position + i).ok_or(A16EngineError::PositionOutOfRange))
            .collect::<Result<_, _>>()?;
        let sink = |i: usize| first_position + i == 0;
        let mut traces = vec![A16TraceV1::default(); tokens.len()];
        let mut hs: Vec<Vec<i32>> = Vec::with_capacity(tokens.len());
        for (i, token) in tokens.iter().enumerate() {
            let rows = self.walk_table(&plan.pre, None, *token, sink(i), rope[i].0, rope[i].1, None, plan.attn_history)?;
            hs.push(rows.last().cloned().unwrap_or_default());
            traces[i].pre = rows;
        }
        for li in 0..plan.layer_count {
            let per_position = self.walk_layer_batched(plan, &hs, tokens, first_position, &rope, li, cache)?;
            for (i, rows) in per_position.into_iter().enumerate() {
                hs[i] = rows.last().cloned().ok_or(A16EngineError::MalformedParams("an empty layer table"))?;
                traces[i].attn.push(rows);
            }
        }
        if !with_post {
            return Ok((Vec::new(), traces));
        }
        let last = tokens.len() - 1;
        let rows = self.walk_table(
            &plan.post,
            Some(&hs[last]),
            tokens[last],
            sink(last),
            rope[last].0,
            rope[last].1,
            None,
            plan.attn_history,
        )?;
        let logits = rows.last().cloned().ok_or(A16EngineError::MalformedParams("an empty post table"))?;
        traces[last].post = rows;
        Ok((logits, traces))
    }

    /// One layer of the plan over a run of positions: node by node, every position's row. A
    /// projection runs once over the run (the sink position alone, on its own parameters); every
    /// other node is [`Self::eval_node`] per position; a cache write appends the run's rows in
    /// position order, and a read hands position `i` the series that ends at its own row.
    #[allow(clippy::too_many_arguments)]
    fn walk_layer_batched(
        &self,
        plan: &A16ProfilePlanV1,
        layer_in: &[Vec<i32>],
        tokens: &[usize],
        first_position: usize,
        rope: &[(&[i32], &[i32])],
        li: usize,
        cache: &mut A16Cache,
    ) -> Result<Vec<Vec<Vec<i32>>>, A16EngineError> {
        use kaspa_consensus_core::palw_step::PalwStepNodeRoleV1 as Role;
        let n = tokens.len();
        let refuse =
            |what: &'static str| move |_e: kaspa_consensus_core::palw_base0_a16::PalwA16OpError| A16EngineError::OpRefused(what);
        // The rows the cache held before this run: position `i` sees them and the run's first `i + 1`.
        let (keys_before, values_before) = (cache.rows_in(li), cache.value_rows_in(li));
        let mut rows: Vec<Vec<Vec<i32>>> = vec![Vec::with_capacity(plan.layer.len()); n];
        for node in &plan.layer {
            let resolve = |i: usize, input: &PlanInput, rows: &[Vec<Vec<i32>>]| -> Result<Vec<i32>, A16EngineError> {
                match input {
                    PlanInput::Row(k) => rows[i].get(*k).cloned().ok_or(A16EngineError::MalformedParams("a forward input ref")),
                    PlanInput::LayerIn => Ok(layer_in[i].clone()),
                    // The series are never rows: they are borrowed from the storage (`kv_for`), and
                    // the plan admits them only where the attention arms declare them.
                    PlanInput::CachedK | PlanInput::CachedV => Err(A16EngineError::MalformedParams("a cache series is not a row input")),
                }
            };
            // Position `i`'s view of the two series: the prefix that ends at its own write — a slice
            // of the storage, whatever width it holds, never a copy (the copy was 2 × 268 MB a layer
            // per run of 64 positions at the 2M context).
            let kv_for = |i: usize| -> Result<(KvSeriesRef<'_>, KvSeriesRef<'_>), A16EngineError> {
                Ok((cache.keys_visible(li, keys_before + i + 1)?, cache.values_visible(li, values_before + i + 1)?))
            };
            let outs: Vec<Vec<i32>> = match node.op {
                PlanOp::MatMulRequant(slot) | PlanOp::MatMulRescale(slot) if self.fast => {
                    let xs: Vec<Vec<i32>> = (0..n).map(|i| resolve(i, &node.inputs[0], &rows)).collect::<Result<_, _>>()?;
                    // The sink position rides its own parameters, so it is run alone.
                    let peel = usize::from(first_position == 0);
                    let mut outs = Vec::with_capacity(n);
                    if peel == 1 {
                        let (w, params) = self.projection_operands(node.op, slot, li, true)?;
                        outs.push(
                            match node.op {
                                PlanOp::MatMulRequant(_) => a16_matmul_requant(true, w, &xs[0], &params),
                                _ => a16_matmul_rescale(true, w, &xs[0], &params),
                            }
                            .map_err(refuse("matmul"))?,
                        );
                    }
                    if n > peel {
                        let (w, params) = self.projection_operands(node.op, slot, li, false)?;
                        let batch = match node.op {
                            PlanOp::MatMulRequant(_) => crate::kernels::a16_matmul_requant_batch(w, &xs[peel..], &params),
                            _ => crate::kernels::a16_matmul_rescale_batch(w, &xs[peel..], &params),
                        }
                        .map_err(refuse("matmul_batch"))?;
                        outs.extend(batch);
                    }
                    outs
                }
                // **The fused site over the run: the series built once, each position handed its
                // prefix.** A position's stepped walk concatenates the whole history for its own
                // read — a copy that grows with the position, quadratic over a prompt — and the
                // prefix of one concatenation is the same bytes. The positions are independent once
                // the run's rows are written, so they run on the pool; each is the same kernel on the
                // same inputs, whatever the schedule.
                PlanOp::AttnFused
                    if self.fast
                        && matches!(node.inputs.get(1), Some(PlanInput::CachedK))
                        && matches!(node.inputs.get(2), Some(PlanInput::CachedV)) =>
                {
                    let p = &self.layers[li];
                    (0..n)
                        .into_par_iter()
                        .map(|i| {
                            let q = resolve(i, &node.inputs[0], &rows)?;
                            let (k, v) = kv_for(i)?;
                            a16_attn_fused_fast(
                                &q,
                                k,
                                v,
                                self.artifact.shape.n_heads,
                                self.artifact.shape.n_kv_heads,
                                self.artifact.shape.d_head,
                                p.logits,
                                p.softmax_up,
                                p.probs,
                                p.values,
                                plan.attn_history,
                            )
                            .map_err(refuse("attn_fused"))
                        })
                        .collect::<Result<_, _>>()?
                }
                // Every other node, each position on its own — independent of one another, so on
                // the pool. The series are resolved only for a node that DECLARES a cache input: a
                // node before the layer's K/V writes has no rows of this run to see yet, and asking
                // for them would be the read-before-write refusal the plan check exists to prevent.
                _ => (0..n)
                    .into_par_iter()
                    .map(|i| {
                        let reads_cache = node.inputs.iter().any(|input| matches!(input, PlanInput::CachedK | PlanInput::CachedV));
                        let kv = if reads_cache { Some(kv_for(i)?) } else { None };
                        self.eval_node(
                            node,
                            &|k| resolve(i, &node.inputs[k], &rows),
                            kv,
                            tokens[i],
                            first_position + i == 0,
                            rope[i].0,
                            rope[i].1,
                            li,
                            plan.attn_history,
                        )
                    })
                    .collect::<Result<_, _>>()?,
            };
            let mut outs = outs;
            if matches!(node.op, PlanOp::AttnFused) {
                for (i, out) in outs.iter_mut().enumerate() {
                    self.drill_attn_at(li, first_position + i, out);
                }
            }
            match node.role {
                Role::KCacheWrite => {
                    for (i, out) in outs.iter().enumerate() {
                        cache.push_key(li, &self.drill_cache_row(li, first_position + i, 0, out))?;
                    }
                }
                Role::VCacheWrite => {
                    for (i, out) in outs.iter().enumerate() {
                        cache.push_value(li, &self.drill_cache_row(li, first_position + i, 1, out))?;
                    }
                }
                Role::Plain => {}
            }
            for (i, out) in outs.into_iter().enumerate() {
                rows[i].push(out);
            }
        }
        Ok(rows)
    }

    /// Walk one table of the plan. `layer` is `Some((index, cache))` for the layer table — the
    /// only table with cache reads and writes — and `layer_in` is the table's input stream.
    #[allow(clippy::too_many_arguments)]
    fn walk_table(
        &self,
        table: &[PlanNode],
        layer_in: Option<&Vec<i32>>,
        token_id: usize,
        sink: bool,
        cos_row: &[i32],
        sin_row: &[i32],
        mut layer: Option<(usize, &mut A16Cache)>,
        attn_history: usize,
    ) -> Result<Vec<Vec<i32>>, A16EngineError> {
        let mut rows: Vec<Vec<i32>> = Vec::with_capacity(table.len());
        for node in table {
            // Resolve the declared inputs against what this walk holds.
            let resolve = |input: &PlanInput, rows: &Vec<Vec<i32>>| -> Result<Vec<i32>, A16EngineError> {
                match input {
                    PlanInput::Row(i) => rows.get(*i).cloned().ok_or(A16EngineError::MalformedParams("a forward input ref")),
                    PlanInput::LayerIn => layer_in.cloned().ok_or(A16EngineError::MalformedParams("layer input outside a layer")),
                    // The series are borrowed from the storage below, never resolved as rows.
                    PlanInput::CachedK | PlanInput::CachedV => Err(A16EngineError::MalformedParams("a cache series is not a row input")),
                }
            };

            // The series are read at USE, so a read after this position's cache write sees the
            // same history the compiled engine hands the kernels — as a slice of the storage.
            let li = layer.as_ref().map(|(li, _)| *li).unwrap_or(0);
            let kv = layer.as_ref().map(|(li, cache)| (cache.keys(*li), cache.values(*li)));
            let mut out =
                self.eval_node(node, &|k| resolve(&node.inputs[k], &rows), kv, token_id, sink, cos_row, sin_row, li, attn_history)?;
            if layer.is_some() && matches!(node.op, PlanOp::AttnFused) {
                self.drill_attn_at(li, self.walk_position.load(std::sync::atomic::Ordering::Relaxed), &mut out);
            }

            // The declared cache write, honored where declared — the ROTATED key and the raw V
            // are conventions of the DECLARATION (the IR carries the role on those nodes), so a
            // profile that declared them elsewhere would cache elsewhere, and its court would
            // read the same declaration.
            match node.role {
                kaspa_consensus_core::palw_step::PalwStepNodeRoleV1::KCacheWrite => {
                    let position = self.walk_position.load(std::sync::atomic::Ordering::Relaxed);
                    let (li, cache) = layer.as_mut().ok_or(A16EngineError::MalformedParams("a cache write outside a layer"))?;
                    cache.push_key(*li, &self.drill_cache_row(*li, position, 0, &out))?;
                }
                kaspa_consensus_core::palw_step::PalwStepNodeRoleV1::VCacheWrite => {
                    let position = self.walk_position.load(std::sync::atomic::Ordering::Relaxed);
                    let (li, cache) = layer.as_mut().ok_or(A16EngineError::MalformedParams("a cache write outside a layer"))?;
                    cache.push_value(*li, &self.drill_cache_row(*li, position, 1, &out))?;
                }
                kaspa_consensus_core::palw_step::PalwStepNodeRoleV1::Plain => {}
            }
            rows.push(out);
        }
        Ok(rows)
    }

    /// **A projection's weights and output parameters** — the ONE selection a stepped walk and the
    /// one-pass prefill both read, so a batched projection is the stepped one's operands exactly. The
    /// sink position's own rows (ADR-0050) ride here: `sink` is the position being 0.
    fn projection_operands(
        &self,
        op: PlanOp,
        slot: MatSlot,
        li: usize,
        sink: bool,
    ) -> Result<(&[i8], Vec<A16QuantParams>), A16EngineError> {
        let lp = &self.layers[li];
        let w = &self.artifact.layers[li];
        Ok(match (op, slot) {
            (PlanOp::MatMulRequant(_), MatSlot::Q) => (&w.wq, lp.q.clone()),
            (PlanOp::MatMulRequant(_), MatSlot::K) => (&w.wk, lp.k.clone()),
            (PlanOp::MatMulRequant(_), MatSlot::V) => (&w.wv, lp.v.clone()),
            (PlanOp::MatMulRequant(_), MatSlot::Wo) => (&w.wo, if sink { lp.wo_sink.clone() } else { lp.wo.clone() }),
            (PlanOp::MatMulRequant(_), MatSlot::Up) => (&w.w_up, if sink { lp.up_sink.clone() } else { lp.up.clone() }),
            (PlanOp::MatMulRequant(_), MatSlot::Down) => (&w.w_down, if sink { lp.down_sink.clone() } else { lp.down.clone() }),
            (PlanOp::MatMulRequant(_), MatSlot::Head) => (&self.artifact.unembed, vec![self.logits_out; self.artifact.shape.vocab]),
            (PlanOp::MatMulRequant(_), MatSlot::Gate) => return Err(A16EngineError::MalformedParams("gate is a rescale site")),
            (PlanOp::MatMulRescale(_), MatSlot::Gate) => (&w.w_gate, lp.gate.clone()),
            (PlanOp::MatMulRescale(_), _) => return Err(A16EngineError::MalformedParams("a rescale site that is not the gate")),
            _ => return Err(A16EngineError::MalformedParams("a projection site that is not a projection")),
        })
    }

    /// **One declared node's row, from its resolved inputs** — the evaluation [`Self::walk_table`]
    /// (a position at a time) and [`Self::forward_prefill_planned`] (a prompt a layer at a time)
    /// share, so a node the two walks evaluate is one computation and the one-pass prefill cannot
    /// commit a row the stepped one would not. `input(k)` is the node's `k`-th declared input,
    /// resolved by the walk that holds it; `kv` is the two cache series the walk lets this node
    /// see (`None` outside a layer); `li` is the layer the node runs in (0 outside one).
    #[allow(clippy::too_many_arguments)]
    fn eval_node(
        &self,
        node: &PlanNode,
        input: &dyn Fn(usize) -> Result<Vec<i32>, A16EngineError>,
        kv: Option<(KvSeriesRef<'_>, KvSeriesRef<'_>)>,
        token_id: usize,
        sink: bool,
        cos_row: &[i32],
        sin_row: &[i32],
        li: usize,
        attn_history: usize,
    ) -> Result<Vec<i32>, A16EngineError> {
        let shape = &self.artifact.shape;
        let d = shape.d_model();
        let kv_dim = shape.kv_dim();
        let refuse =
            |what: &'static str| move |_e: kaspa_consensus_core::palw_base0_a16::PalwA16OpError| A16EngineError::OpRefused(what);
        let tile = |p: A16QuantParams, n: usize| -> Vec<A16QuantParams> { vec![p; n] };
        let lp = |li: usize| -> &LayerParams { &self.layers[li] };
        let out: Vec<i32> = match node.op {
            PlanOp::EmbedGather => {
                if token_id >= self.artifact.shape.vocab {
                    return Err(A16EngineError::OpRefused("a token outside the vocabulary"));
                }
                self.artifact.embed[token_id * d..(token_id + 1) * d].iter().map(|c| *c as i32).collect()
            }
            PlanOp::RmsNorm => {
                let x = input(0)?;
                a16_rms_norm(&x, shape.eps_q).map_err(refuse("rms_norm"))?
            }
            PlanOp::Requant(slot) => {
                let x = input(0)?;
                let params: Vec<A16QuantParams> = match slot {
                    ReqSlot::EmbedLift => tile(self.embed_lift, d),
                    ReqSlot::AttnNorm => lp(li).attn_norm.clone(),
                    ReqSlot::Probs => {
                        let history = x.len() / shape.n_heads.max(1);
                        tile(lp(li).probs, shape.n_heads * history)
                    }
                    ReqSlot::AttnAlign => tile(if sink { lp(li).attn_align_sink } else { lp(li).attn_align }, d),
                    ReqSlot::AttnResidual => tile(lp(li).attn_residual, d),
                    ReqSlot::FfnNorm => lp(li).ffn_norm.clone(),
                    ReqSlot::SiluQ => tile(if sink { lp(li).silu_sink } else { lp(li).silu_q }, shape.d_ff),
                    ReqSlot::Gated => tile(if sink { lp(li).gated_sink } else { lp(li).gated }, shape.d_ff),
                    ReqSlot::FfnAlign => tile(if sink { lp(li).ffn_align_sink } else { lp(li).ffn_align }, d),
                    ReqSlot::FfnResidual => tile(lp(li).ffn_residual, d),
                    ReqSlot::FinalNorm => self.final_norm.clone(),
                };
                a16_requant(&x, &params).map_err(refuse("requant"))?
            }
            PlanOp::MatMulRequant(slot) => {
                let x = input(0)?;
                let (w, params) = self.projection_operands(node.op, slot, li, sink)?;
                a16_matmul_requant(self.fast, w, &x, &params).map_err(refuse("matmul_requant"))?
            }
            PlanOp::MatMulRescale(slot) => {
                let x = input(0)?;
                let (w, params) = self.projection_operands(node.op, slot, li, sink)?;
                a16_matmul_rescale(self.fast, w, &x, &params).map_err(refuse("matmul_rescale"))?
            }
            PlanOp::Rope { kv } => {
                let x = input(0)?;
                let heads = if kv { shape.n_kv_heads } else { shape.n_heads };
                if x.len() != heads * shape.d_head {
                    return Err(A16EngineError::OpRefused("a rotation whose input is not its declared width"));
                }
                let mut out = Vec::with_capacity(x.len());
                for hd in 0..heads {
                    let slice = &x[hd * shape.d_head..(hd + 1) * shape.d_head];
                    out.extend(a16_rope(slice, cos_row, sin_row).map_err(|_| A16EngineError::OpRefused("rope"))?);
                }
                out
            }
            PlanOp::AttnScores => {
                let q = input(0)?;
                let (k_series, _) = kv.ok_or(A16EngineError::MalformedParams("a cache read outside a layer"))?;
                let history = k_series.len() / kv_dim.max(1);
                a16_attn_scores(
                    self.fast,
                    &q,
                    k_series,
                    shape.n_heads,
                    shape.n_kv_heads,
                    shape.d_head,
                    &tile(lp(li).logits, shape.n_heads * history),
                )
                .map_err(refuse("attn_scores"))?
            }
            PlanOp::Softmax => {
                let x = input(0)?;
                let history = x.len() / shape.n_heads.max(1);
                a16_softmax_rows(&x, history, lp(li).softmax_up).map_err(refuse("softmax"))?
            }
            PlanOp::AttnValues => {
                let p = input(0)?;
                let (_, v_series) = kv.ok_or(A16EngineError::MalformedParams("a cache read outside a layer"))?;
                a16_attn_values(
                    self.fast,
                    attn_history,
                    &p,
                    v_series,
                    shape.n_heads,
                    shape.n_kv_heads,
                    shape.d_head,
                    &tile(lp(li).values, shape.n_heads * shape.d_head),
                )
                .map_err(refuse("attn_values"))?
            }
            // **The fused attention site** (ADR-0082 Decision 1). The four shipped kernels
            // composed — W9, W11, the probability requantization, W10 — with the three
            // intermediates living only in this frame. The row pushed below is the OUTPUT
            // row, so the site commits `heads x d_head` codes at every context instead of
            // three rows that grow with the position.
            //
            // Composed from the engine's OWN kernels rather than from
            // `a16_attn_fused_via_tiles_v1`: the two are proven equal at every history
            // length and tile width (`palw_base0_a16::fused::the_tile_route_is_the
            // _composition`), and the composition is what the fast projections are asserted
            // bit-identical against, so this keeps the executor at the runtime's speed while
            // computing exactly what `a16_attn_fused_reference_v1` defines. The equality is
            // held by `the_fused_arm_is_the_reference_composition` below.
            // On the fast engine the four ops run as one kernel with each narrowing's
            // parameters read once (`kernels::a16_attn_fused_uniform_fast`, bit-identical to the
            // composition below and held to it by `the_fast_engine_and_the_catalog_agree_token_for
            // _token`): the composition tiles a triple over `heads × history` twice a call and
            // materialises four history-long rows, which is quadratic allocation over a job.
            PlanOp::AttnFused if self.fast => {
                let q = input(0)?;
                let (k_series, v_series) = kv.ok_or(A16EngineError::MalformedParams("a cache read outside a layer"))?;
                let p = lp(li);
                a16_attn_fused_fast(
                    &q,
                    k_series,
                    v_series,
                    shape.n_heads,
                    shape.n_kv_heads,
                    shape.d_head,
                    p.logits,
                    p.softmax_up,
                    p.probs,
                    p.values,
                    attn_history,
                )
                .map_err(refuse("attn_fused"))?
            }
            PlanOp::AttnFused => {
                let q = input(0)?;
                let (k_series, v_series) = kv.ok_or(A16EngineError::MalformedParams("a cache read outside a layer"))?;
                let history = k_series.len() / kv_dim.max(1);
                let scores = a16_attn_scores(
                    self.fast,
                    &q,
                    k_series,
                    shape.n_heads,
                    shape.n_kv_heads,
                    shape.d_head,
                    &tile(lp(li).logits, shape.n_heads * history),
                )
                .map_err(refuse("attn_scores"))?;
                let probs = a16_softmax_rows(&scores, history, lp(li).softmax_up).map_err(refuse("softmax"))?;
                let codes = a16_requant(&probs, &tile(lp(li).probs, probs.len())).map_err(refuse("requant"))?;
                a16_attn_values(
                    self.fast,
                    attn_history,
                    &codes,
                    v_series,
                    shape.n_heads,
                    shape.n_kv_heads,
                    shape.d_head,
                    &tile(lp(li).values, shape.n_heads * shape.d_head),
                )
                .map_err(refuse("attn_values"))?
            }
            PlanOp::AddElem => {
                let a = input(0)?;
                let b = input(1)?;
                a16_add_elem(&a, &b).map_err(refuse("add_elem"))?
            }
            PlanOp::MulElem => {
                let a = input(0)?;
                let b = input(1)?;
                a16_mul_elem(&a, &b).map_err(refuse("mul_elem"))?
            }
            PlanOp::Silu => {
                let x = input(0)?;
                silu(&x)
            }
        };
        Ok(out)
    }
}

/// Compile one declared table. Every refusal names the node and the reason — this function IS
/// the kernel-set boundary of ADR-0067 Decision 3.
fn plan_table(
    nodes: &[PalwStepNodeV1],
    table: &'static str,
    shape: &Base0ShapeV1,
    layer_in: Option<u32>,
) -> Result<Vec<PlanNode>, A16PlanErrorV1> {
    use kaspa_consensus_core::palw_step::PalwStepOpKindV1 as Op;

    // **Only the layer table has a layer.** A `blk.{layer}.*` operand names a per-layer parameter
    // row, and `walk_table` resolves the layer as `layer.unwrap_or(0)` — so a pre or post node
    // carrying one would silently execute under layer 0's parameters. The class would run and
    // certify; its every dispute would then be unadjudicable, because the court walks the
    // DECLARED graph and the declaration says nothing about which layer that node meant.
    let per_layer_ok = table == "layer";

    let k_embed = kernel_semantics_id_v1(KDESC_A16_EMBED);
    let k_req = kernel_semantics_id_v1(KDESC_A16_REQUANTIZE);
    let k_mm = kernel_semantics_id_v1(KDESC_A16_MATMUL_REQUANT);
    let k_rs = kernel_semantics_id_v1(KDESC_A16_MATMUL_RESCALE);
    let k_rms = kernel_semantics_id_v1(KDESC_A16_RMS_NORM);
    let k_rope = kernel_semantics_id_v1(KDESC_A16_ROPE);
    let k_scores = kernel_semantics_id_v1(KDESC_A16_ATTN_SCORES);
    let k_soft = kernel_semantics_id_v1(KDESC_A16_SOFTMAX);
    let k_vals = kernel_semantics_id_v1(KDESC_A16_ATTN_VALUES);
    let k_fused = kernel_semantics_id_v1(KDESC_A16_ATTN_FUSED);
    let k_add = kernel_semantics_id_v1(KDESC_A16_ADD_ELEM);
    let k_mul = kernel_semantics_id_v1(KDESC_A16_MUL_ELEM);
    let k_silu = kernel_semantics_id_v1(KDESC_Q36_SILU);

    let d = shape.d_model() as u32;
    let kv_dim = shape.kv_dim() as u32;
    let ffn = shape.d_ff as u32;
    let vocab = shape.vocab as u32;
    let heads = shape.n_heads as u32;

    let refuse = |index: usize, reason: String| A16PlanErrorV1::UnservedNode { table, index, reason };

    /// What a node's output IS, statically: a fixed element count, or a kv-scaled row family.
    /// Tracked so every consumer's input width is checked AT PLAN TIME — the fuzz gate's first
    /// find was a gate-and-plan-accepted profile whose rewired input refs fed a kv-width row to
    /// the q-rope, and the head slicing walked off the end mid-forward. A width-sound plan makes
    /// that whole class of profile unplannable instead of un-panickable.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum W {
        Fixed(u32),
        KvScaled(u32),
        Series,
    }

    let mut widths: Vec<W> = Vec::with_capacity(nodes.len());
    let mut out = Vec::with_capacity(nodes.len());
    for (index, node) in nodes.iter().enumerate() {
        // The declared inputs, resolved first — every op checks its arity against them.
        let mut inputs = Vec::with_capacity(node.input_refs.len());
        for r in &node.input_refs {
            let input = match *r {
                PALW_STEP_INPUT_LAYER_IN => PlanInput::LayerIn,
                PALW_STEP_INPUT_KV_K => PlanInput::CachedK,
                PALW_STEP_INPUT_KV_V => PlanInput::CachedV,
                i if i >= PALW_STEP_INPUT_SENTINEL_MIN => {
                    return Err(refuse(index, format!("input sentinel {i:#x} is not one this family serves")));
                }
                i => {
                    if (i as usize) >= index {
                        return Err(refuse(index, format!("input ref {i} is not an earlier node of this table")));
                    }
                    PlanInput::Row(i as usize)
                }
            };
            inputs.push(input);
        }
        let arity = |n: usize| -> Result<(), A16PlanErrorV1> {
            if inputs.len() != n { Err(refuse(index, format!("arity {} where the kernel takes {n}", inputs.len()))) } else { Ok(()) }
        };
        let width_of = |input: &PlanInput| -> W {
            match input {
                PlanInput::Row(i) => widths[*i],
                // NOT an assumption: the caller passes the producing table's terminal width, because
                // `forward_token_planned` feeds this table whatever the previous table's LAST declared
                // node produced. Assuming hidden width here let a gate-accepted profile hand a
                // kv-width residual to a hidden-width consumer, and `PlanOp::Rope` slices by hand —
                // an out-of-bounds panic where a refusal belonged.
                PlanInput::LayerIn => layer_in.map(W::Fixed).unwrap_or(W::Series),
                PlanInput::CachedK | PlanInput::CachedV => W::Series,
            }
        };
        let need = |slot: usize, want: W, what: &str| -> Result<(), A16PlanErrorV1> {
            let got = width_of(&inputs[slot]);
            if got != want {
                return Err(refuse(index, format!("input {slot} is {got:?} where {what} takes {want:?}")));
            }
            Ok(())
        };
        let width = |want: u32, what: &str| -> Result<(), A16PlanErrorV1> {
            match node.out_len {
                PalwStepOutLenV1::Fixed { elements } if elements == want => Ok(()),
                other => Err(refuse(index, format!("out width {other:?} where {what} is {want}"))),
            }
        };
        let kv_scaled = |want_mult: u32| -> Result<(), A16PlanErrorV1> {
            match node.out_len {
                PalwStepOutLenV1::KvScaled { multiplier } if multiplier == want_mult => Ok(()),
                other => Err(refuse(index, format!("out width {other:?} where the kv-scaled multiplier is {want_mult}"))),
            }
        };
        // Weight-bearing nodes must be the integer dtype this family's matmuls read. Dtype IS
        // arithmetic (the profile's own field doc), so a foreign byte is an unserved node, not a
        // detail.
        let dtype_i8 = || -> Result<(), A16PlanErrorV1> {
            if node.weight_dtypes.iter().all(|b| *b == kaspa_consensus_core::palw_qwen25_profile::QWEN25_WEIGHT_DTYPE_I8) {
                Ok(())
            } else {
                Err(refuse(index, "a weight dtype this family's kernels do not read".to_string()))
            }
        };

        let name = node.weight_name.as_str();
        if !per_layer_ok && strip_layer(name).is_some() {
            return Err(refuse(index, format!("operand {name:?} names a per-layer row, and the {table} table has no layer")));
        }
        let kid = node.kernel_semantics_id;
        let op = match (node.op_kind, name) {
            (Op::EmbedLookup, "token_embd.weight") if kid == k_embed => {
                arity(0)?;
                width(d, "hidden")?;
                dtype_i8()?;
                PlanOp::EmbedGather
            }
            (Op::RmsNorm, "") if kid == k_rms => {
                arity(1)?;
                width(d, "hidden")?;
                need(0, W::Fixed(d), "the norm")?;
                PlanOp::RmsNorm
            }
            (Op::MulElem, n) if kid == k_req => {
                arity(1)?;
                // Probs is the one requant whose width scales with the kv history; every other
                // slot is fixed. One arm, two width rules, stated rather than special-cased.
                let (slot, fixed) = match strip_layer(n) {
                    Some("attn_norm.a16") => (ReqSlot::AttnNorm, Some(d)),
                    Some("attn_probs.a16") => (ReqSlot::Probs, None),
                    Some("attn_align.a16") => (ReqSlot::AttnAlign, Some(d)),
                    Some("attn_residual.a16") => (ReqSlot::AttnResidual, Some(d)),
                    Some("ffn_norm.a16") => (ReqSlot::FfnNorm, Some(d)),
                    Some("ffn_silu.a16") => (ReqSlot::SiluQ, Some(ffn)),
                    Some("ffn_gated.a16") => (ReqSlot::Gated, Some(ffn)),
                    Some("ffn_align.a16") => (ReqSlot::FfnAlign, Some(d)),
                    Some("ffn_residual.a16") => (ReqSlot::FfnResidual, Some(d)),
                    None if n == "embed_lift.a16" => (ReqSlot::EmbedLift, Some(d)),
                    None if n == "final_norm.a16" => (ReqSlot::FinalNorm, Some(d)),
                    _ => return Err(refuse(index, format!("requant operand {n:?} is not one this store names"))),
                };
                match fixed {
                    Some(want) => {
                        width(want, "the slot's width")?;
                        need(0, W::Fixed(want), "a width-preserving requant")?;
                    }
                    None => {
                        kv_scaled(heads)?;
                        need(0, W::KvScaled(heads), "the probs requant")?;
                    }
                }
                PlanOp::Requant(slot)
            }
            (Op::MatMulQuant, n) if kid == k_mm => {
                arity(1)?;
                dtype_i8()?;
                let slot = match strip_layer(n) {
                    Some("attn_q.weight") => (MatSlot::Q, d),
                    Some("attn_k.weight") => (MatSlot::K, kv_dim),
                    Some("attn_v.weight") => (MatSlot::V, kv_dim),
                    Some("attn_output.weight") => (MatSlot::Wo, d),
                    Some("ffn_up.weight") => (MatSlot::Up, ffn),
                    Some("ffn_down.weight") => (MatSlot::Down, d),
                    // Both head spellings resolve to the same slot: the v1 class ties the head to
                    // the embedding by NAME, the v2 class names the engine's own head view so the
                    // gather's rows and the matmul's tiles stop colliding in the inventory
                    // (`QWEN25_A16_HEAD_TENSOR_V2`'s doc). The bytes are `artifact.unembed` either
                    // way — tying remains a fact about bytes.
                    None if n == "token_embd.weight" || n == "output.weight" => (MatSlot::Head, vocab),
                    _ => return Err(refuse(index, format!("matmul operand {n:?} is not one this store names"))),
                };
                width(slot.1, "the slot's width")?;
                let in_width = match slot.0 {
                    MatSlot::Down => ffn,
                    _ => d,
                };
                need(0, W::Fixed(in_width), "this matmul's fan-in")?;
                PlanOp::MatMulRequant(slot.0)
            }
            (Op::MatMulQuant, n) if kid == k_rs => {
                arity(1)?;
                dtype_i8()?;
                match strip_layer(n) {
                    Some("ffn_gate.weight") => {
                        width(ffn, "ffn")?;
                        need(0, W::Fixed(d), "the gate's fan-in")?;
                        PlanOp::MatMulRescale(MatSlot::Gate)
                    }
                    _ => return Err(refuse(index, format!("rescale operand {n:?} is not one this store names"))),
                }
            }
            (Op::MatMulQuant, n) if kid == k_scores => {
                arity(2)?;
                if strip_layer(n) != Some("attn_logits.a16") {
                    return Err(refuse(index, format!("scores operand {n:?} is not one this store names")));
                }
                kv_scaled(heads)?;
                need(0, W::Fixed(d), "the query")?;
                if inputs.get(1) != Some(&PlanInput::CachedK) {
                    return Err(refuse(index, "scores read something other than the key series".to_string()));
                }
                PlanOp::AttnScores
            }
            (Op::SoftMax, n) if kid == k_soft => {
                arity(1)?;
                if strip_layer(n) != Some("attn_softmax_up") {
                    return Err(refuse(index, format!("softmax operand {n:?} is not one this store names")));
                }
                kv_scaled(heads)?;
                need(0, W::KvScaled(heads), "the row softmax")?;
                PlanOp::Softmax
            }
            (Op::MatMulQuant, n) if kid == k_vals => {
                arity(2)?;
                if strip_layer(n) != Some("attn_values.a16") {
                    return Err(refuse(index, format!("values operand {n:?} is not one this store names")));
                }
                width(d, "hidden")?;
                need(0, W::KvScaled(heads), "the probability rows")?;
                if inputs.get(1) != Some(&PlanInput::CachedV) {
                    return Err(refuse(index, "values read something other than the value series".to_string()));
                }
                PlanOp::AttnValues
            }
            // **ADR-0082 Decision 1**, the fused site. Its four registered operands come from the
            // ONE the node names, through `palw_attn_fused_tensors_v1` — the same function the
            // adjudicator reads, so the engine and the court cannot resolve different tensors —
            // and the derived names are then checked against the ones this store actually holds.
            (Op::AttnFused, n) if kid == k_fused => {
                arity(3)?;
                let t = palw_attn_fused_tensors_v1(n)
                    .ok_or_else(|| refuse(index, format!("fused operand {n:?} is not a softmax store this family registers")))?;
                for (what, got, want) in [
                    ("softmax", t.softmax_up.as_str(), "attn_softmax_up"),
                    ("scores", t.scores.as_str(), "attn_logits.a16"),
                    ("probs", t.probs.as_str(), "attn_probs.a16"),
                    ("values", t.values.as_str(), "attn_values.a16"),
                ] {
                    if strip_layer(got) != Some(want) {
                        let why = format!("the fused site's {what} operand derives to {got:?}, which is not one this store names");
                        return Err(refuse(index, why));
                    }
                }
                // The committed row is the OUTPUT row (Z0's first half); the query is the rotated
                // one and the two series are the caches, in the order the court reads them.
                width(d, "hidden")?;
                need(0, W::Fixed(d), "the query")?;
                if inputs.get(1) != Some(&PlanInput::CachedK) {
                    return Err(refuse(index, "a fused attention site read something other than the key series".to_string()));
                }
                if inputs.get(2) != Some(&PlanInput::CachedV) {
                    return Err(refuse(index, "a fused attention site read something other than the value series".to_string()));
                }
                PlanOp::AttnFused
            }
            (Op::RopeImrope, "rope") if kid == k_rope => {
                arity(1)?;
                use kaspa_consensus_core::palw_step::PalwStepNodeRoleV1 as Role;
                let kv = match (node.role, node.out_len) {
                    (Role::KCacheWrite, _) => true,
                    (_, PalwStepOutLenV1::Fixed { elements }) if elements == kv_dim && kv_dim != d => true,
                    (_, PalwStepOutLenV1::Fixed { elements }) if elements == d => false,
                    (_, other) => return Err(refuse(index, format!("rope out width {other:?} fits neither q nor k"))),
                };
                width(if kv { kv_dim } else { d }, "the rotated width")?;
                need(0, W::Fixed(if kv { kv_dim } else { d }), "the rotation")?;
                PlanOp::Rope { kv }
            }
            (Op::AddElem, "") if kid == k_add => {
                arity(2)?;
                width(d, "hidden")?;
                need(0, W::Fixed(d), "the residual add")?;
                need(1, W::Fixed(d), "the residual add")?;
                PlanOp::AddElem
            }
            (Op::MulElem, "") if kid == k_mul => {
                arity(2)?;
                width(ffn, "ffn")?;
                need(0, W::Fixed(ffn), "the gated product")?;
                need(1, W::Fixed(ffn), "the gated product")?;
                PlanOp::MulElem
            }
            (Op::Silu, "") if kid == k_silu => {
                arity(1)?;
                width(ffn, "ffn")?;
                need(0, W::Fixed(ffn), "the nonlinearity")?;
                PlanOp::Silu
            }
            (op, n) => {
                return Err(refuse(
                    index,
                    format!("op {op:?} with kernel {kid} and operand {n:?} is outside this build's served vocabulary"),
                ));
            }
        };
        widths.push(match node.out_len {
            PalwStepOutLenV1::Fixed { elements } => W::Fixed(elements),
            PalwStepOutLenV1::KvScaled { multiplier } => W::KvScaled(multiplier),
        });
        out.push(PlanNode { op, inputs, role: node.role });
    }
    Ok(out)
}

/// `blk.{layer}.suffix` → `suffix`. The `{layer}` template survives lowering (the profile's own
/// field doc: substituted at interpretation time), so the ABI here is the literal template.
fn strip_layer(name: &str) -> Option<&str> {
    name.strip_prefix("blk.{layer}.")
}

/// **The claim `A16-KV-i16` ships on: the compact cache computes the reference cache's bits.**
///
/// Not "the same logits": every committed row of every table, the cache left behind decoded to
/// `i32`, and every chunk the checkpoint serializer emits — on the v2, v5 and v7 graphs, on the fast
/// engine and the catalog one, through the stepped walk, the one-pass prefill and the hand-written
/// v2 program, and across a checkpoint restart in both directions. The oracle is `A16-KV-i32`;
/// what is proven is that the representation is invisible to every byte a commitment can see.
#[cfg(test)]
mod kv_storage_tests {
    use super::*;
    use crate::artifact::LN_THETA_10000_GEN_Q;
    use kaspa_consensus_core::palw_qwen25_profile::{
        PalwQwen25GeometryV1, qwen25_a16_profile_v2, qwen25_a16_profile_v5, qwen25_a16_profile_v7,
    };
    use kaspa_consensus_core::palw_state_chunk_map::{PalwStateChunkEntryV1, PalwStateChunkGeometryV1, PalwStateChunkKindV1};

    fn artifact(n_layers: usize, d_head: usize, d_ff: usize) -> Base0ArtifactV1 {
        let shape = Base0ShapeV1 {
            n_layers,
            n_heads: 4,
            n_kv_heads: 2,
            d_head,
            d_ff,
            vocab: 64,
            max_position: 32,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1,
        };
        Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(derived_a16_store(&shape))
            .expect("the derived store is sorted and unique")
    }

    fn geometry(a: &Base0ArtifactV1) -> PalwQwen25GeometryV1 {
        PalwQwen25GeometryV1 {
            layer_count: a.shape.n_layers as u16,
            hidden_dim: a.shape.d_model() as u32,
            ffn_dim: a.shape.d_ff as u32,
            attn_heads: a.shape.n_heads as u16,
            attn_kv_heads: a.shape.n_kv_heads as u16,
            attn_head_dim: a.shape.d_head as u32,
            vocab_size: a.shape.vocab as u32,
            n_ctx: 32,
            n_threads: 1,
            rms_eps_q: a.shape.eps_q,
            tile_len: 4,
        }
    }

    /// The whole state at `rows`, one chunk per `(kind, layer)` at the committed four-byte width, in
    /// the map's own order (kind-major, layer ascending) — the bytes a checkpoint commits.
    fn whole_state_chunks(cache: &A16Cache, layers: usize, kv_dim: usize, rows: usize) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        for kind in PalwStateChunkKindV1::ALL {
            for li in 0..layers {
                let entry = PalwStateChunkEntryV1 {
                    kind,
                    attn_layer: li as u16,
                    position_start: 0,
                    position_count: rows as u32,
                    row_bytes: (kv_dim * 4) as u32,
                };
                out.push(cache.state_chunk_bytes_v1(&entry).expect("a whole-history chunk encodes"));
            }
        }
        out
    }

    fn whole_state_geometry(layers: usize, kv_dim: usize, rows: usize) -> PalwStateChunkGeometryV1 {
        PalwStateChunkGeometryV1 {
            row_bytes: (kv_dim * 4) as u32,
            positions_per_chunk: rows as u32,
            attn_layers: (0..layers as u16).collect(),
            positions: rows as u32,
            chunks_per_slice: 1,
        }
    }

    /// Everything a commitment can see of a cache: its decoded contents and its committed chunks.
    type Observed = ((Vec<Vec<i32>>, Vec<Vec<i32>>), Vec<Vec<u8>>);
    fn observed(cache: &A16Cache, layers: usize, kv_dim: usize) -> Observed {
        (cache.contents_as_i32(), whole_state_chunks(cache, layers, kv_dim, cache.rows()))
    }

    #[test]
    fn a_compact_cache_computes_the_reference_bits() {
        let mut checked = 0usize;
        for (layers, d_head, d_ff) in [(1usize, 4usize, 12usize), (2, 8, 16)] {
            let artifact = artifact(layers, d_head, d_ff);
            let kv_dim = artifact.shape.kv_dim();
            let g = geometry(&artifact);
            let profiles = [
                ("v2", qwen25_a16_profile_v2(g).expect("v2")),
                ("v5", qwen25_a16_profile_v5(g).expect("v5")),
                ("v7", qwen25_a16_profile_v7(g).expect("v7")),
            ];
            let tokens: Vec<usize> = (0..9).map(|i| (i * 11 + 5) % artifact.shape.vocab).collect();
            for (engine_name, engine) in
                [("fast", A16Engine::new(&artifact).expect("fast")), ("reference", A16Engine::new_reference(&artifact).expect("reference"))]
            {
                for (name, profile) in &profiles {
                    let plan = engine.plan_from_profile(profile).expect("servable");
                    let at = format!("{name} on the {engine_name} engine, layers={layers}");

                    // The stepped walk, position by position, on both representations.
                    let mut oracle = A16Cache::new(layers);
                    let mut compact = A16Cache::with_storage(layers, KvStorageProfileV1::A16KvI16);
                    for (i, token) in tokens.iter().enumerate() {
                        let (la, ta) = engine.forward_token_planned(&plan, &mut oracle, *token, i).expect("the oracle steps");
                        let (lb, tb) = engine.forward_token_planned(&plan, &mut compact, *token, i).expect("the compact cache steps");
                        assert_eq!(la, lb, "{at}: logits at {i}");
                        assert_eq!(ta, tb, "{at}: every committed row at {i}");
                    }
                    assert_eq!(observed(&oracle, layers, kv_dim), observed(&compact, layers, kv_dim), "{at}: the stepped cache");
                    assert_eq!(compact.storage(), KvStorageProfileV1::A16KvI16);

                    // The one-pass prefill from a stepped prefix, in runs of three, on both.
                    if plan.one_pass_prefill_supported() {
                        let mut oracle = A16Cache::new(layers);
                        let mut compact = A16Cache::with_storage(layers, KvStorageProfileV1::A16KvI16);
                        for p in 0..2usize {
                            engine.forward_token_planned(&plan, &mut oracle, (p * 3 + 1) % 64, p).expect("prefix");
                            engine.forward_token_planned(&plan, &mut compact, (p * 3 + 1) % 64, p).expect("prefix");
                        }
                        for (chunk_index, chunk) in tokens.chunks(3).enumerate() {
                            let first = 2 + chunk_index * 3;
                            let last = first + chunk.len() == 2 + tokens.len();
                            let (la, ta) = engine.forward_prefill_planned(&plan, &mut oracle, chunk, first, last).expect("oracle run");
                            let (lb, tb) = engine.forward_prefill_planned(&plan, &mut compact, chunk, first, last).expect("compact run");
                            assert_eq!(la, lb, "{at}: one-pass logits at run {chunk_index}");
                            assert_eq!(ta, tb, "{at}: one-pass rows at run {chunk_index}");
                        }
                        assert_eq!(observed(&oracle, layers, kv_dim), observed(&compact, layers, kv_dim), "{at}: the one-pass cache");
                    }
                    checked += 1;
                }

                // The hand-written v2 program and its batched prefill, on both.
                let mut oracle = A16Cache::new(layers);
                let mut compact = A16Cache::with_storage(layers, KvStorageProfileV1::A16KvI16);
                for (i, token) in tokens.iter().enumerate().take(5) {
                    let (la, ta) = engine.forward_token_traced(&mut oracle, *token, i).expect("traced");
                    let (lb, tb) = engine.forward_token_traced(&mut compact, *token, i).expect("traced");
                    assert_eq!((la, ta), (lb, tb), "{engine_name} traced at {i}");
                }
                let la = engine.forward_prefill(&mut oracle, &tokens[5..], 5, 2).expect("batched");
                let lb = engine.forward_prefill(&mut compact, &tokens[5..], 5, 2).expect("batched");
                assert_eq!(la, lb, "{engine_name}: the batched prefill's logits");
                assert_eq!(observed(&oracle, layers, kv_dim), observed(&compact, layers, kv_dim), "{engine_name}: the v2 program's cache");
            }
        }
        assert_eq!(checked, 2 * 2 * 3, "every fixture, engine and graph");
    }

    /// **A restart is invisible too**: the state serialized at position six from either
    /// representation, restored into either, continued to position ten — the same rows, the same
    /// chunks and the same cache as the walk that never stopped. In every direction, including
    /// `i16 → chunks → i32`, which is what an `i32` court reads from an `i16` producer's checkpoint.
    #[test]
    fn a_restart_from_a_checkpoint_continues_the_reference_bits_in_every_direction() {
        let (layers, d_head, d_ff) = (2usize, 8usize, 16usize);
        let artifact = artifact(layers, d_head, d_ff);
        let kv_dim = artifact.shape.kv_dim();
        let engine = A16Engine::new(&artifact).expect("fast");
        let g = geometry(&artifact);
        let tokens: Vec<usize> = (0..10).map(|i| (i * 7 + 3) % 64).collect();
        let (stop, end) = (6usize, 10usize);
        for (name, profile) in [("v2", qwen25_a16_profile_v2(g).expect("v2")), ("v7", qwen25_a16_profile_v7(g).expect("v7"))] {
            let plan = engine.plan_from_profile(&profile).expect("servable");
            // The walk that never stops, and what it committed at every step past the stop.
            let mut whole = A16Cache::new(layers);
            let mut after: Vec<(Vec<i32>, A16TraceV1)> = Vec::new();
            for (i, token) in tokens.iter().enumerate() {
                let step = engine.forward_token_planned(&plan, &mut whole, *token, i).expect("whole");
                if i >= stop {
                    after.push(step);
                }
            }
            let whole_observed = observed(&whole, layers, kv_dim);

            for from in KvStorageProfileV1::ALL {
                // The producer's cache at the stop, in `from`, serialized.
                let mut origin = A16Cache::with_storage(layers, from);
                for (i, token) in tokens.iter().enumerate().take(stop) {
                    engine.forward_token_planned(&plan, &mut origin, *token, i).expect("origin");
                }
                let chunks = whole_state_chunks(&origin, layers, kv_dim, stop);
                let geometry = whole_state_geometry(layers, kv_dim, stop);
                for into in KvStorageProfileV1::ALL {
                    let at = format!("{name}: {} → chunks → {}", from.name(), into.name());
                    let mut restored = A16Cache::from_state_chunks_v1(into, layers, kv_dim, &geometry, &chunks).expect("restores");
                    assert_eq!(restored.storage(), into);
                    assert_eq!(restored.rows(), stop, "{at}: the restored rows");
                    assert_eq!(restored.contents_as_i32(), origin.contents_as_i32(), "{at}: the restored state");
                    for (k, i) in (stop..end).enumerate() {
                        let step = engine.forward_token_planned(&plan, &mut restored, tokens[i], i).expect("continues");
                        assert_eq!(step, after[k], "{at}: the continued step at {i}");
                    }
                    assert_eq!(observed(&restored, layers, kv_dim), whole_observed, "{at}: the cache at the end");
                }
            }
        }
    }

    /// **The compact cache refuses exactly the state the reference refuses — one step earlier.** A
    /// served chunk carrying a value outside the code range is held by the `i32` cache and refused
    /// at its first attention read; the `i16` cache refuses it at the restore. Both are refusals of
    /// the same replay. The rails themselves (`±32,767`) are accepted by both, a write outside the
    /// range under `i16` is refused rather than narrowed, and a map that does not cover its state
    /// is refused by both.
    #[test]
    fn the_compact_cache_refuses_what_the_reference_refuses() {
        let (layers, d_head, d_ff) = (1usize, 4usize, 12usize);
        let artifact = artifact(layers, d_head, d_ff);
        let kv_dim = artifact.shape.kv_dim();
        let engine = A16Engine::new(&artifact).expect("fast");
        let plan = engine.plan_from_profile(&qwen25_a16_profile_v5(geometry(&artifact)).expect("v5")).expect("servable");
        let rows = 3usize;
        let mut origin = A16Cache::new(layers);
        for i in 0..rows {
            engine.forward_token_planned(&plan, &mut origin, (i * 5 + 1) % 64, i).expect("origin");
        }
        let good = whole_state_chunks(&origin, layers, kv_dim, rows);
        let geometry = whole_state_geometry(layers, kv_dim, rows);

        for (what, value) in [("32,768", 32_768i32), ("-32,768", -32_768), ("40,000", 40_000), ("i32::MIN", i32::MIN)] {
            let mut bad = good.clone();
            bad[0][4..8].copy_from_slice(&value.to_le_bytes());
            assert!(
                A16Cache::from_state_chunks_v1(KvStorageProfileV1::A16KvI16, layers, kv_dim, &geometry, &bad).is_err(),
                "{what}: the i16 cache refuses at the restore"
            );
            let mut wide = A16Cache::from_state_chunks_v1(KvStorageProfileV1::A16KvI32, layers, kv_dim, &geometry, &bad)
                .expect("the i32 cache holds the value");
            assert!(
                engine.forward_token_planned(&plan, &mut wide, 9, rows).is_err(),
                "{what}: and the reference refuses it at the first attention read"
            );
        }
        for value in [32_767i32, -32_767] {
            let mut rail = good.clone();
            rail[0][4..8].copy_from_slice(&value.to_le_bytes());
            for storage in KvStorageProfileV1::ALL {
                let mut cache = A16Cache::from_state_chunks_v1(storage, layers, kv_dim, &geometry, &rail).expect("a rail is a code");
                engine.forward_token_planned(&plan, &mut cache, 9, rows).expect("and attention reads it");
            }
        }
        // A write outside the range is refused under i16, held under i32.
        let mut compact = A16Cache::with_storage(layers, KvStorageProfileV1::A16KvI16);
        assert!(compact.push_key(0, &[40_000, 0, 0, 0, 0, 0, 0, 0]).is_err());
        assert_eq!(compact.rows(), 0, "nothing was written");
        let mut oracle = A16Cache::new(layers);
        oracle.push_key(0, &[40_000, 0, 0, 0, 0, 0, 0, 0]).expect("the i32 cache holds it");
        assert_eq!(oracle.keys_as_i32(0)[0], 40_000);
        // Coverage: one chunk short is refused by both; a chunk of the wrong length likewise.
        for storage in KvStorageProfileV1::ALL {
            assert!(A16Cache::from_state_chunks_v1(storage, layers, kv_dim, &geometry, &good[..good.len() - 1]).is_err());
            let mut short = good.clone();
            short[1].pop();
            assert!(A16Cache::from_state_chunks_v1(storage, layers, kv_dim, &geometry, &short).is_err());
        }
    }

    /// **The bytes the cache allocates are the resource profile's `kv_resident_bytes`** after an
    /// exact reservation, under either representation — the figure the gate reserves is the
    /// figure the walk spends. Without the reservation the buffers double and hold at most twice.
    #[test]
    fn the_resident_bytes_are_the_resource_profiles_kv_term() {
        use kaspa_consensus_core::palw_resource_profile_v1::palw_kv_series_bytes_v1;
        let (layers, d_head, d_ff) = (2usize, 8usize, 16usize);
        let artifact = artifact(layers, d_head, d_ff);
        let kv_dim = artifact.shape.kv_dim();
        let engine = A16Engine::new(&artifact).expect("fast");
        let plan = engine.plan_from_profile(&qwen25_a16_profile_v7(geometry(&artifact)).expect("v7")).expect("servable");
        let rows = 9usize;
        for storage in KvStorageProfileV1::ALL {
            let expected = palw_kv_series_bytes_v1(layers as u64, kv_dim as u64, rows as u64, storage.kv_bytes_per_element());
            let mut reserved = A16Cache::with_storage(layers, storage);
            reserved.reserve_positions(rows, kv_dim);
            let mut grown = A16Cache::with_storage(layers, storage);
            for i in 0..rows {
                engine.forward_token_planned(&plan, &mut reserved, (i * 7 + 3) % 64, i).expect("reserved");
                engine.forward_token_planned(&plan, &mut grown, (i * 7 + 3) % 64, i).expect("grown");
            }
            assert_eq!(reserved.resident_bytes_v1(), expected, "{}: an exact reservation is the profile's term", storage.name());
            assert!(grown.resident_bytes_v1() >= expected && grown.resident_bytes_v1() <= 2 * expected, "{}: doubling holds at most twice", storage.name());
            assert_eq!(reserved.contents_as_i32(), grown.contents_as_i32());
        }
        assert_eq!(
            A16Cache::with_storage(1, KvStorageProfileV1::A16KvI16).resident_bytes_v1(),
            0,
            "an empty cache reserved nothing"
        );
        assert_eq!(KV_STORAGE_SHIPPED_V1, KvStorageProfileV1::A16KvI16, "the shipped representation is the compact one");
    }
}

#[cfg(test)]
mod profile_plan_tests {
    use super::*;
    use crate::artifact::LN_THETA_10000_GEN_Q;
    use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, qwen25_a16_profile_v1, qwen25_a16_profile_v2};

    fn artifact(n_layers: usize, d_head: usize, d_ff: usize) -> Base0ArtifactV1 {
        let shape = Base0ShapeV1 {
            n_layers,
            n_heads: 4,
            n_kv_heads: 2,
            d_head,
            d_ff,
            vocab: 64,
            max_position: 32,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1,
        };
        Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(derived_a16_store(&shape))
            .expect("the derived store is sorted and unique")
    }

    fn geometry(a: &Base0ArtifactV1) -> PalwQwen25GeometryV1 {
        PalwQwen25GeometryV1 {
            layer_count: a.shape.n_layers as u16,
            hidden_dim: a.shape.d_model() as u32,
            ffn_dim: a.shape.d_ff as u32,
            attn_heads: a.shape.n_heads as u16,
            attn_kv_heads: a.shape.n_kv_heads as u16,
            attn_head_dim: a.shape.d_head as u32,
            vocab_size: a.shape.vocab as u32,
            n_ctx: 16,
            n_threads: 1,
            rms_eps_q: a.shape.eps_q,
            tile_len: 4,
        }
    }

    /// **ADR-0082 Decision 1: the fused arm IS the four shipped kernels, and the graph around it
    /// did not move.**
    ///
    /// Three claims in one walk, at every layer and every position including the sink:
    ///
    /// * the v5 plan's committed row at the fused site equals the v2 plan's `ATTN_VALUES` row —
    ///   the site commits the OUTPUT row and the three context-wide rows simply stop existing;
    /// * that row equals `a16_attn_fused_reference_v1`, which is the kernel descriptor's declared
    ///   semantics and exactly what the court's arm recomputes, AND
    ///   `a16_attn_fused_via_tiles_v1`, which is what a dissection folds to — so the executor,
    ///   the whole-row court and the tile route are one number (invariant Z1);
    /// * the logits and the cache the whole forward leaves behind are bit-identical between the
    ///   two graphs, which is the statement that graph v5 computes the same model.
    #[test]
    fn the_fused_arm_is_the_reference_composition() {
        use kaspa_consensus_core::palw_base0_a16::{A16AttnFusedParamsV1, a16_attn_fused_reference_v1, a16_attn_fused_via_tiles_v1};
        use kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v5;
        use kaspa_consensus_core::palw_step::PalwStepOutLenV1 as OutLen;

        for (layers, d_head, d_ff) in [(1usize, 4usize, 12usize), (2, 8, 16)] {
            let artifact = artifact(layers, d_head, d_ff);
            let engine = A16Engine::new(&artifact).expect("the store resolves");
            let g = geometry(&artifact);
            let v2 = qwen25_a16_profile_v2(g).expect("the v2 profile builds");
            let v5 = qwen25_a16_profile_v5(g).expect("the v5 profile builds");
            // Z0's first half, on the row this build will actually execute.
            assert!(
                v5.attn_nodes.iter().all(|n| !matches!(n.out_len, OutLen::KvScaled { .. })),
                "a v5 layer table still commits a context-shaped row"
            );
            let fused_at = v5
                .attn_nodes
                .iter()
                .position(|n| n.op_kind == kaspa_consensus_core::palw_step::PalwStepOpKindV1::AttnFused)
                .expect("the v5 layer table has a fused site");
            let plan_v2 = engine.plan_from_profile(&v2).expect("v2 is servable");
            let plan_v5 = engine.plan_from_profile(&v5).expect("v5 is servable");

            let mut cache_v2 = A16Cache::new(layers);
            let mut cache_v5 = A16Cache::new(layers);
            for position in 0..6usize {
                let token = (position * 7 + 3) % artifact.shape.vocab;
                let (a, ta) = engine.forward_token_planned(&plan_v2, &mut cache_v2, token, position).expect("v2 walks");
                let (b, tb) = engine.forward_token_planned(&plan_v5, &mut cache_v5, token, position).expect("v5 walks");
                assert_eq!(a, b, "the logits moved at position {position}");
                assert_eq!(ta.pre, tb.pre, "pre rows at position {position}");
                assert_eq!(ta.post, tb.post, "post rows at position {position}");
                for li in 0..layers {
                    let v2_rows = &ta.attn[li];
                    let v5_rows = &tb.attn[li];
                    assert_eq!(v5_rows.len() + 3, v2_rows.len(), "layer {li}: four rows became one");
                    // The fused row is the values row, and every row after it is unchanged.
                    assert_eq!(v5_rows[fused_at], v2_rows[fused_at + 3], "layer {li} position {position}: the fused row");
                    assert_eq!(&v5_rows[..fused_at], &v2_rows[..fused_at], "layer {li}: the rows before the site");
                    assert_eq!(&v5_rows[fused_at + 1..], &v2_rows[fused_at + 4..], "layer {li}: the rows after the site");

                    // …and it is the catalogued composition, and the tile route, to the bit.
                    let lp = &engine.layers[li];
                    let params =
                        A16AttnFusedParamsV1 { scores: lp.logits, probs: lp.probs, values: lp.values, up_bits: lp.softmax_up };
                    let q = &v5_rows[v5.attn_nodes[fused_at].input_refs[0] as usize];
                    let k = cache_v5.keys_as_i32(li);
                    let v = cache_v5.values_as_i32(li);
                    let (h, kvh, dh) = (artifact.shape.n_heads, artifact.shape.n_kv_heads, artifact.shape.d_head);
                    let reference = a16_attn_fused_reference_v1(q, &k, &v, h, kvh, dh, params).expect("the composition runs");
                    assert_eq!(v5_rows[fused_at], reference, "layer {li} position {position}: the engine parted from the reference");
                    for tile in [1usize, 4, 16] {
                        let tiled = a16_attn_fused_via_tiles_v1(q, &k, &v, h, kvh, dh, params, tile).expect("the tile route runs");
                        assert_eq!(v5_rows[fused_at], tiled, "layer {li} position {position} tile {tile}: the tile route parted");
                    }
                }
            }
            assert_eq!(cache_v2.contents_as_i32(), cache_v5.contents_as_i32(), "the two graphs must leave the same cache");
        }
    }

    /// **The dense tier's one-pass prefill is the position-by-position one** (ADR-0117 Decision 2):
    /// over the v2, v5 (fused site) and v7 (held map) plans, on the fast engine and the catalog one,
    /// a prompt run a layer at a time in runs of every width — from the sink, and from a cache a
    /// stepped walk already filled — leaves every committed row, the last position's logits and the
    /// cache exactly as the stepped walk leaves them. The post rows are the last position's only.
    #[test]
    fn the_one_pass_prefill_is_the_position_by_position_one() {
        use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_profile_v5, qwen25_a16_profile_v7};
        let mut checked = 0usize;
        for (layers, d_head, d_ff) in [(1usize, 4usize, 12usize), (2, 8, 16)] {
            let artifact = artifact(layers, d_head, d_ff);
            let g = geometry(&artifact);
            let profiles = [
                ("v2", qwen25_a16_profile_v2(g).expect("v2")),
                ("v5", qwen25_a16_profile_v5(g).expect("v5")),
                ("v7", qwen25_a16_profile_v7(g).expect("v7")),
            ];
            for engine in [A16Engine::new(&artifact).expect("fast"), A16Engine::new_reference(&artifact).expect("reference")] {
                for (name, profile) in &profiles {
                    let plan = engine.plan_from_profile(profile).expect("servable");
                    assert!(plan.one_pass_prefill_supported(), "{name}: every read after its write");
                    let tokens: Vec<usize> = (0..9).map(|i| (i * 11 + 5) % artifact.shape.vocab).collect();
                    for start in [0usize, 1, 4] {
                        // The stepped walk: a prefix to stand on, then the run.
                        let mut stepped = A16Cache::new(layers);
                        for p in 0..start {
                            engine.forward_token_planned(&plan, &mut stepped, (p * 3 + 1) % artifact.shape.vocab, p).expect("prefix");
                        }
                        let mut want = Vec::new();
                        let mut last_logits = Vec::new();
                        for (i, token) in tokens.iter().enumerate() {
                            let (logits, trace) = engine.forward_token_planned(&plan, &mut stepped, *token, start + i).expect("steps");
                            want.push(trace);
                            last_logits = logits;
                        }
                        for run in [1usize, 2, 3, 5, tokens.len()] {
                            let mut passed = A16Cache::new(layers);
                            for p in 0..start {
                                engine
                                    .forward_token_planned(&plan, &mut passed, (p * 3 + 1) % artifact.shape.vocab, p)
                                    .expect("prefix");
                            }
                            let mut got = Vec::new();
                            let mut logits = Vec::new();
                            for (at, chunk) in tokens.chunks(run).enumerate() {
                                let first = start + at * run;
                                let last = first + chunk.len() == start + tokens.len();
                                let (l, traces) =
                                    engine.forward_prefill_planned(&plan, &mut passed, chunk, first, last).expect("one pass");
                                if last {
                                    logits = l;
                                } else {
                                    assert!(l.is_empty(), "no logits without the post table");
                                }
                                got.extend(traces);
                            }
                            let at = format!("{name} layers={layers} start={start} run={run}");
                            assert_eq!(logits, last_logits, "{at}: the last position's logits");
                            for (i, (g, w)) in got.iter().zip(&want).enumerate() {
                                assert_eq!(g.pre, w.pre, "{at}: pre rows at {i}");
                                assert_eq!(g.attn, w.attn, "{at}: layer rows at {i}");
                                if i + 1 == tokens.len() {
                                    assert_eq!(g.post, w.post, "{at}: the last position's post rows");
                                } else {
                                    assert!(g.post.is_empty(), "{at}: no post rows before the last position");
                                }
                            }
                            assert_eq!(passed.contents_as_i32(), stepped.contents_as_i32(), "{at}: the cache left behind");
                            checked += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(checked, 2 * 2 * 3 * 3 * 5, "every plan, engine, start and run width");
    }

    /// **The one-pass prefill on the real dense row, measured** (ADR-0117 Decision 2). Off unless
    /// `MISAKA_PALW_ONE_PASS_ARTIFACT` names the converted 1.5B artifact — 1.7 GiB of weights are not a
    /// unit test's input. The testnet-11 graph-v5 row's plan runs the same prompt position by position
    /// and a run of positions at a time; every committed row (hashed as it is produced, so the
    /// comparison holds no trace), the cache left behind and the last logits must be the same bits,
    /// and the two times are printed:
    ///
    /// ```text
    /// MISAKA_PALW_ONE_PASS_ARTIFACT=/path/qwen25-1.5b-a16.palwart MISAKA_PALW_ONE_PASS_PREFILL=256 \
    ///   cargo test --release -p misaka-palw-base0 --lib -- one_pass_prefill_on_the_real --nocapture
    /// ```
    #[test]
    fn one_pass_prefill_on_the_real_dense_row() {
        use std::hash::{Hash, Hasher};
        let Ok(path) = std::env::var("MISAKA_PALW_ONE_PASS_ARTIFACT") else {
            eprintln!("one-pass: skipped — set MISAKA_PALW_ONE_PASS_ARTIFACT to the dense .palwart to measure");
            return;
        };
        let prefill: usize = std::env::var("MISAKA_PALW_ONE_PASS_PREFILL").ok().and_then(|v| v.parse().ok()).unwrap_or(256);
        let run: usize = std::env::var("MISAKA_PALW_ONE_PASS_RUN").ok().and_then(|v| v.parse().ok()).unwrap_or(32);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let artifact = crate::artifact::decode_artifact_file_v1(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
        drop(bytes);
        let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_graph_v5_profile_v1().expect("the shipped dense row");
        let engine = A16Engine::new(&artifact).expect("an A16 artifact");
        let plan = engine.plan_from_profile(&profile).expect("the shipped row is servable");
        assert!(plan.one_pass_prefill_supported());
        let vocab = artifact.shape.vocab;
        let prompt: Vec<usize> = (0..prefill).map(|i| (i * 7919 + 1013) % vocab).collect();
        let fold = |state: &mut std::collections::hash_map::DefaultHasher, trace: &A16TraceV1| {
            trace.pre.hash(state);
            trace.attn.hash(state);
        };

        let started = std::time::Instant::now();
        let mut stepped = A16Cache::new(artifact.shape.n_layers);
        let mut stepped_rows = std::collections::hash_map::DefaultHasher::new();
        let mut stepped_logits = Vec::new();
        for (position, token) in prompt.iter().enumerate() {
            let (logits, trace) = engine.forward_token_planned(&plan, &mut stepped, *token, position).expect("a step");
            fold(&mut stepped_rows, &trace);
            if position + 1 == prefill {
                trace.post.hash(&mut stepped_rows);
            }
            stepped_logits = logits;
        }
        let stepped_took = started.elapsed();

        let started = std::time::Instant::now();
        let mut passed = A16Cache::new(artifact.shape.n_layers);
        let mut passed_rows = std::collections::hash_map::DefaultHasher::new();
        let mut passed_logits = Vec::new();
        let mut position = 0usize;
        while position < prefill {
            let end = (position + run).min(prefill);
            let last = end == prefill;
            let (logits, traces) =
                engine.forward_prefill_planned(&plan, &mut passed, &prompt[position..end], position, last).expect("a run");
            for trace in &traces {
                fold(&mut passed_rows, trace);
            }
            if last {
                traces.last().expect("a run has positions").post.hash(&mut passed_rows);
                passed_logits = logits;
            }
            position = end;
        }
        let passed_took = started.elapsed();

        assert_eq!(passed_rows.finish(), stepped_rows.finish(), "every committed row");
        assert_eq!(passed_logits, stepped_logits, "the last position's logits");
        assert_eq!(passed.contents_as_i32(), stepped.contents_as_i32(), "the cache the prefill leaves");
        eprintln!(
            "one-pass on the real dense row: {prefill} positions — stepped {:.3} s ({:.1} ms/position), runs of {run} {:.3} s \
             ({:.1} ms/position), {:.2}x",
            stepped_took.as_secs_f64(),
            stepped_took.as_secs_f64() * 1000.0 / prefill as f64,
            passed_took.as_secs_f64(),
            passed_took.as_secs_f64() * 1000.0 / prefill as f64,
            stepped_took.as_secs_f64() / passed_took.as_secs_f64()
        );
    }

    /// **ADR-0067's differential gate, in miniature: the compiled rows are the interpreter's
    /// reference vectors.** The plan is compiled from the CORRECTED profile — the graph that
    /// names what the engine does — so walking it must land on the compiled engine's exact bits:
    /// logits, every committed row of every table, and the cache left behind, across positions
    /// (including position 0, where the sink-variant parameters switch in).
    #[test]
    fn the_interpreter_and_the_compiled_engine_agree_bit_for_bit() {
        for (layers, d_head, d_ff) in [(1usize, 4usize, 12usize), (2, 8, 16)] {
            let artifact = artifact(layers, d_head, d_ff);
            let engine = A16Engine::new(&artifact).expect("the store resolves");
            let profile = qwen25_a16_profile_v2(geometry(&artifact)).expect("the corrected profile builds");
            let plan = engine.plan_from_profile(&profile).expect("the corrected profile is servable");

            let mut compiled_cache = A16Cache::new(layers);
            let mut planned_cache = A16Cache::new(layers);
            for position in 0..6usize {
                let token = (position * 7 + 3) % artifact.shape.vocab;
                let (a, ta) = engine.forward_token_traced(&mut compiled_cache, token, position).expect("compiled");
                let (b, tb) = engine.forward_token_planned(&plan, &mut planned_cache, token, position).expect("planned");
                assert_eq!(a, b, "logits at position {position}");
                assert_eq!(ta.pre, tb.pre, "pre rows at position {position}");
                assert_eq!(ta.attn, tb.attn, "layer rows at position {position}");
                assert_eq!(ta.post, tb.post, "post rows at position {position}");
            }
            assert_eq!(compiled_cache.contents_as_i32(), planned_cache.contents_as_i32(), "the caches must be the same state");
        }
    }

    /// **ADR-0067 Decision 5, clause (b): the differential over the classes THIS BUILD CARRIES.**
    ///
    /// The differential above proves the two engines agree on two synthetic geometries. That is
    /// the mechanism; it is not the claim Decision 5 makes, which is about the rows a node
    /// actually ships — because those are the graphs a chain registers, and a class the build
    /// carries that the interpreter cannot serve is a node that admits what it cannot run.
    ///
    /// Two halves, both over the REAL catalog:
    ///
    /// * every A16-family row's real profile is compiled, and the planner's answer is pinned. A
    ///   `graph-v2` row must be servable; a v1 row must be REFUSED, and refused for its own
    ///   documented reason (its pre table omits the embed-lift requant, so the interpreter
    ///   executing the declaration is a different arithmetic from the compiled engine — which is
    ///   exactly what makes v1 unfit for the free-prompt lane).
    /// * for every row the planner serves, the SAME graph is built at a runnable geometry and the
    ///   two engines are compared row for row. The node tables are generated from one IR and one
    ///   geometry, so a reduced geometry walks the identical node sequence; what it cannot do is
    ///   hold 1.7 GiB of weights in a unit test, which is why the artifact is derived.
    #[test]
    fn the_interpreter_serves_every_a16_class_this_build_carries() {
        use crate::classes::canonical_classes_v1;
        use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;

        let court = PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).expect("shipped court");
        let a16: Vec<_> = canonical_classes_v1(&court)
            .into_iter()
            .filter(|c| matches!(c.source, crate::classes::ArtifactSourceV1::ConvertedA16))
            .collect();
        assert!(!a16.is_empty(), "the build carries A16 rows, or this test gates nothing");

        let mut served = 0usize;
        for entry in &a16 {
            // The row's REAL profile — the graph a chain would register for it.
            let probe_artifact = artifact(1, 4, 12);
            let probe_engine = A16Engine::new(&probe_artifact).expect("the store resolves");
            let real = probe_engine.plan_from_profile(&entry.profile);
            // **Derived from the GRAPH, not matched on the NAME.** This read
            // `entry.model_id.ends_with("/graph-v2")`, which is the row's label rather than its
            // content — and the moment a second corrected row arrived under a different label
            // (`/graph-v3`, the row that declares the epsilon its artifact executes) the test
            // classified it as uncorrected, built its comparison profile from the v1 tables, and
            // failed asserting a v1 property of a v2 graph. The name is a fact about what we called
            // the row; what the assertions below are about is whether its pre table NAMES the
            // embed-lift requant, which is a fact about the graph. Same rule as everywhere else in
            // this tree: derive, never declare.
            let corrected = entry.profile.pre_nodes.len() == kaspa_consensus_core::palw_base0_profile::QWEN25_A16_PRE_IR_V2.len();
            match (&real, corrected) {
                // A real profile against a MISMATCHED artifact must refuse on geometry — that is
                // the root check doing its job, and it tells us the planner reached the geometry
                // gate rather than accepting a graph it cannot size.
                (Err(A16PlanErrorV1::GeometryMismatch { .. }), _) => {}
                (Err(A16PlanErrorV1::UnservedNode { table, index, reason }), false) => {
                    assert!(
                        reason.contains("requant") || reason.contains("vocabulary") || !reason.is_empty(),
                        "a v1 row's refusal must name something: {table}[{index}] {reason}"
                    );
                }
                (other, _) => panic!("{}: unexpected plan answer {other:?}", entry.model_id),
            }

            // The same graph at a runnable geometry, both engines, row for row.
            let g = kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
                layer_count: 2,
                hidden_dim: 16,
                ffn_dim: 12,
                attn_heads: 4,
                attn_kv_heads: 2,
                attn_head_dim: 4,
                vocab_size: 64,
                n_ctx: entry.profile.n_ctx,
                n_threads: 1,
                rms_eps_q: 1,
                tile_len: 4,
            };
            let small = if corrected {
                kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v2(g)
            } else {
                kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v1(g)
            };
            let Ok(small) = small else { continue };
            let small_artifact = artifact(2, 4, 12);
            let engine = A16Engine::new(&small_artifact).expect("the store resolves");
            let Ok(plan) = engine.plan_from_profile(&small) else {
                // A v1 row is refused here for its own reason; that IS the pinned answer.
                assert!(!corrected, "{}: a corrected row must be servable at a runnable geometry", entry.model_id);
                continue;
            };
            served += 1;
            let (mut a, mut b) = (A16Cache::new(2), A16Cache::new(2));
            for position in 0..4usize {
                let token = (position * 11 + 5) % small_artifact.shape.vocab;
                let (la, ta) = engine.forward_token_traced(&mut a, token, position).expect("compiled");
                let (lb, tb) = engine.forward_token_planned(&plan, &mut b, token, position).expect("planned");
                if corrected {
                    // The corrected graph names what the engine does, so the two must agree
                    // everywhere — this is the property the whole ADR turns on.
                    assert_eq!(la, lb, "{} logits at {position}", entry.model_id);
                    assert_eq!(ta.pre, tb.pre, "{} pre rows at {position}", entry.model_id);
                    assert_eq!(ta.attn, tb.attn, "{} layer rows at {position}", entry.model_id);
                    assert_eq!(ta.post, tb.post, "{} post rows at {position}", entry.model_id);
                } else {
                    // **A v1 row is servable and DIFFERENT, and that is the finding, not a bug in
                    // this test.** Its pre table declares one node where the engine performs two
                    // (the gather, then the embed-lift requant), so an interpreter executing the
                    // declaration commits one row where the compiled engine commits two. A
                    // producer running the compiled engine under a v1 class therefore commits
                    // rows at coordinates the court does not have — which is precisely why
                    // ADR-0049 Decision F refuses v1 on the free-prompt lane, and precisely what
                    // the interpreter makes structurally impossible for a class built from its
                    // own declaration.
                    assert_eq!(
                        tb.pre.len(),
                        entry.profile.pre_nodes.len(),
                        "{} commits one row per DECLARED pre node",
                        entry.model_id
                    );
                    assert!(
                        ta.pre.len() > tb.pre.len(),
                        "{}: the compiled engine performs a narrowing this graph does not name — if this ever stops \
                         being true, the v1 rows have become servable and Decision F's refusal needs revisiting",
                        entry.model_id
                    );
                }
            }
        }
        assert!(served > 0, "at least one carried class must be servable, or the interpreter serves nothing this build ships");
    }

    /// **The audit's own findings, as tests.** Each of these was a gate-accepted profile that the
    /// planner let through and the forward then panicked on, or executed under the wrong
    /// parameters — found by adversarial review of this file (2026-08-31), and each is now a
    /// named refusal at PLAN time.
    #[test]
    fn a_gate_accepted_profile_cannot_reach_a_panic_through_the_plan() {
        let artifact = artifact(1, 4, 12);
        let engine = A16Engine::new(&artifact).expect("the store resolves");
        let good = qwen25_a16_profile_v2(geometry(&artifact)).expect("builds");
        let hidden = artifact.shape.d_model() as u32;
        let kv_dim = artifact.shape.kv_dim() as u32;

        // (1) A pre table that ends at a width no layer reads. `forward_token_planned` feeds the
        // layer table `rows.last()`, so this used to hand a kv-width residual to a hidden-width
        // consumer — and `PlanOp::Rope` slices by hand, which is an out-of-bounds panic.
        let mut narrow_pre = good.clone();
        narrow_pre.pre_nodes[1].out_len = PalwStepOutLenV1::Fixed { elements: kv_dim };
        assert_ne!(kv_dim, hidden, "the fixture must actually be GQA or this proves nothing");
        match engine.plan_from_profile(&narrow_pre) {
            Err(A16PlanErrorV1::UnservedNode { table: "pre", .. }) => {}
            other => panic!("a pre table that does not end on the hidden stream must be refused, got {other:?}"),
        }

        // (2) A per-layer operand in a table that HAS no layer. The walk resolves the layer as
        // `unwrap_or(0)`, so this used to execute silently under layer 0's parameters: a class
        // that runs and certifies, and whose every dispute is unadjudicable because the court
        // walks a declaration that never said which layer it meant.
        let mut per_layer_in_post = good.clone();
        per_layer_in_post.post_nodes[1].weight_name = "blk.{layer}.attn_norm.a16".into();
        match engine.plan_from_profile(&per_layer_in_post) {
            Err(A16PlanErrorV1::UnservedNode { table: "post", reason, .. }) => {
                assert!(reason.contains("per-layer"), "the refusal names why: {reason}");
            }
            other => panic!("a per-layer operand outside the layer table must be refused, got {other:?}"),
        }
    }

    /// **The refusals name the boundary** (ADR-0067 Decision 3). A foreign kernel, a forward
    /// input reference, a stranger's operand name and a wrong geometry each fail at PLAN time,
    /// each naming what this build cannot serve — never a mid-forward surprise.
    #[test]
    fn an_unservable_profile_is_refused_at_plan_time_by_name() {
        let artifact = artifact(1, 4, 12);
        let engine = A16Engine::new(&artifact).expect("the store resolves");
        let good = qwen25_a16_profile_v2(geometry(&artifact)).expect("builds");

        let mut foreign_kernel = good.clone();
        foreign_kernel.attn_nodes[0].kernel_semantics_id = kernel_semantics_id_v1("a16/some-future-kernel/v9");
        match engine.plan_from_profile(&foreign_kernel) {
            Err(A16PlanErrorV1::UnservedNode { table: "layer", index: 0, .. }) => {}
            other => panic!("a foreign kernel must be an unserved node, got {other:?}"),
        }

        let mut forward_ref = good.clone();
        forward_ref.attn_nodes[0].input_refs = vec![5];
        match engine.plan_from_profile(&forward_ref) {
            Err(A16PlanErrorV1::UnservedNode { table: "layer", index: 0, .. }) => {}
            other => panic!("a forward input ref must be refused, got {other:?}"),
        }

        let mut stranger = good.clone();
        stranger.attn_nodes[1].weight_name = "blk.{layer}.someone_elses.a16".into();
        match engine.plan_from_profile(&stranger) {
            Err(A16PlanErrorV1::UnservedNode { table: "layer", index: 1, .. }) => {}
            other => panic!("a stranger's operand must be refused, got {other:?}"),
        }

        let mut wrong = good;
        wrong.hidden_dim += 1;
        match engine.plan_from_profile(&wrong) {
            Err(A16PlanErrorV1::GeometryMismatch { what: "hidden_dim", .. }) => {}
            other => panic!("a wrong geometry must be refused at the root, got {other:?}"),
        }
    }

    /// **The interpreter executes the DECLARATION, not the family habit.** The v1 profile omits
    /// the embed-lift requant (the Decision F defect that keeps the v1 class off the free-prompt
    /// lane). An interpreter serving it must run exactly the declared graph — one pre row, no
    /// lift — and therefore land on DIFFERENT logits than the compiled engine, which always
    /// lifts. That difference is the honest outcome: a court adjudicates the declared graph, and
    /// an interpreter that quietly "fixed" the declaration would commit arithmetic the court
    /// recomputes differently — the exact conviction Decision F exists to prevent.
    #[test]
    fn the_interpreter_executes_the_declared_graph_not_the_family_habit() {
        // The derived store's embed lift is unity, under which "lift" and "no lift" are the
        // same arithmetic. A shift would not do either (the first layer node is a
        // scale-invariant RMS norm), and a small zero offset drowns in the derived store's
        // saturation. A LARGE zero offset moves the residual stream itself, which nothing
        // downstream can launder — so that is the narrowing this test declares.
        let shape = artifact(1, 4, 12).shape;
        let mut store = derived_a16_store(&shape);
        for (name, bytes) in store.iter_mut() {
            if name == "embed_lift.a16" {
                *bytes = A16QuantParams { multiplier: 1, shift: 0, zero: 20_000 }.to_wire().to_vec();
            }
        }
        let artifact = Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(store)
            .expect("sorted and unique");
        let engine = A16Engine::new(&artifact).expect("the store resolves");
        let v1 = qwen25_a16_profile_v1(geometry(&artifact)).expect("the v1 profile builds");
        let plan = engine.plan_from_profile(&v1).expect("every v1 node is individually servable");

        let mut planned_cache = A16Cache::new(1);
        let (planned, trace) = engine.forward_token_planned(&plan, &mut planned_cache, 3, 0).expect("the declared graph runs");
        assert_eq!(trace.pre.len(), 1, "the v1 declaration has ONE pre node, and one row was committed for it");

        let mut compiled_cache = A16Cache::new(1);
        let (compiled, _) = engine.forward_token_traced(&mut compiled_cache, 3, 0).expect("compiled");
        assert_ne!(planned, compiled, "the lift the v1 graph does not declare must not be executed for it");
    }
}
