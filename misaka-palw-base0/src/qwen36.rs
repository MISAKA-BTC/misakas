//! **The Qwen3.6 hybrid engine — thirty GatedDeltaNet arms, ten gated-attention arms, forty MoE
//! blocks.**
//!
//! `Base0ArtifactV1` is a dense decoder container: seven weight tensors per layer, one shape for
//! all of them. Qwen3.6 is not that. Its forty layers alternate 3:1 between a linear-attention arm
//! carrying a recurrent state and a softmax arm with an output gate, every one of them followed by
//! a 256-expert mixture whose weights are 92 % of the model. A struct with named fields for that
//! would have sixty of them and would need a new field for the next architecture.
//!
//! So the container here is a **named store**, which is what GGUF is and what the court's weight
//! oracle already addresses (`operand_bytes(tensor_name, layer, …)`). Weights are `i8` under
//! template names; the A16 `(multiplier, shift, zero)` triples are the same store the dense tier
//! uses. Two consequences that are the reason for the choice:
//!
//! * a tensor is a slice, so the store can become offsets into a memory map without the engine
//!   changing — and at 35 B parameters a memory map is not an optimisation, it is the only way the
//!   weights fit on a machine that has less RAM than the model;
//! * a court opening names a tensor the same way the engine reads it, so there is no translation
//!   layer to get wrong.
//!
//! # What is here and what is not
//!
//! Every op is [`kaspa_consensus_core::palw_qwen36_ops`] or the A16 tier. Nothing new is defined,
//! and nothing is fused: the output gate, the L2 norm, the convolution and the recurrence are four
//! nodes, because a fused node is a node a bisection cannot land inside.
//!
//! The **converter** is not here. This runs an artifact; turning a checkpoint into one is a
//! separate pipeline, and it is the piece that decides fidelity.

use crate::artifact::ArtifactError;
use crate::rope::RopeTableV1;
// **The projections and the attention arms go through `kernels`, not through the catalog ops.**
//
// Every one of them is asserted bit-identical to the op it replaces (`kernels`' own differentials,
// plus `engine_a16`'s whole-forward comparison), which is the property ADR-0040 Decision E exists
// to provide: lanes, tiles and threads cannot change an integer result. Reading the catalog ops
// here instead would make a 40-layer forward roughly thirteen times slower for the same bits.
use crate::kernels::{
    a16_attn_scores_fast as a16_attn_scores, a16_attn_values_fast as a16_attn_values, a16_matmul_requant_fast as a16_matmul_requant,
    a16_matmul_rescale_fast as a16_matmul_rescale,
};
use kaspa_consensus_core::palw_base0_a16::{A16QuantParams, a16_add_elem, a16_requant, a16_rms_norm, a16_softmax_rows};
use kaspa_consensus_core::palw_base0_ops::silu;
use kaspa_consensus_core::palw_qwen36_ops::{
    Qwen36GdnParamsV1, Qwen36GdnStateV1, q36_decay, q36_gate_apply, q36_gdn_step, q36_l2_norm, q36_moe_combine, q36_mul_wide,
    q36_rescale_row, q36_rms_norm_wide, q36_rope_partial, q36_router_topk, q36_sigmoid_gate, q36_ssm_conv,
};
use kaspa_hashes::Hash64;
use std::collections::BTreeMap;

/// Which arm a layer carries. `config.json` calls these `linear_attention` and `full_attention`
/// and lists them per layer rather than deriving them from an interval, so this does too: a model
/// that changes the pattern is a different `layer_types`, not a different code path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Qwen36LayerKind {
    /// GatedDeltaNet: a four-tap causal convolution, an L2-normalized key, and a recurrent state.
    LinearAttention,
    /// Grouped-query softmax attention with per-head QK-norm, partial rotation and an output gate.
    FullAttention,
}

/// The geometry, from `config.json`'s `text_config`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Qwen36ShapeV1 {
    pub layer_types: Vec<Qwen36LayerKind>,
    pub d_model: usize,
    /// Full attention: 16 query heads over 2 kv heads, head_dim 256.
    pub n_heads: usize,
    pub n_kv_heads: usize,
    pub head_dim: usize,
    /// `partial_rotary_factor · head_dim`. Qwen3.6 rotates 64 of every head's 256 lanes and
    /// carries the other 192 untouched; rotating all of them is a different model.
    pub rotary_dim: usize,
    /// Linear attention: 16 key heads and 32 value heads, head_dim 128.
    pub linear_k_heads: usize,
    pub linear_v_heads: usize,
    pub linear_head_dim: usize,
    pub conv_kernel: usize,
    /// MoE: 256 experts, 8 routed per token, intermediate 512, plus one always-on shared expert.
    pub n_experts: usize,
    pub experts_per_token: usize,
    pub moe_dim: usize,
    pub shared_dim: usize,
    pub vocab: usize,
    pub max_position: usize,
    pub eps_q: i64,
    /// The widening `softmax_shifted` applies to a router row. Class data: a router's logits are
    /// no more confined to Qk than an attention logit is.
    pub router_up_bits: u8,
}

impl Qwen36ShapeV1 {
    pub fn n_layers(&self) -> usize {
        self.layer_types.len()
    }
    pub fn kv_dim(&self) -> usize {
        self.n_kv_heads * self.head_dim
    }
    pub fn linear_k_dim(&self) -> usize {
        self.linear_k_heads * self.linear_head_dim
    }
    pub fn linear_v_dim(&self) -> usize {
        self.linear_v_heads * self.linear_head_dim
    }

    /// **The qwen3moe members: every layer is full attention.** Qwen3-Coder-30B-A3B and kin —
    /// no GDN recurrence anywhere in the stack.
    pub fn is_full_attention_only(&self) -> bool {
        self.layer_types.iter().all(|k| *k == Qwen36LayerKind::FullAttention)
    }

    /// **Whether the attention output rides a sigmoid gate** — DERIVED, not stored, so the
    /// artifact format did not change: in this lineage the hybrid (Qwen3.6) fuses a gate into its
    /// q projection and the full-attention-only members (qwen3moe) are plain q/k/v/o. If a
    /// gateless hybrid or a gated all-attention model ever ships, this becomes a stored field —
    /// bump the file magic then, not now.
    pub fn attn_output_gate(&self) -> bool {
        !self.is_full_attention_only()
    }

    /// **Whether the mixture carries the always-on shared expert** beside the routed ones.
    pub fn has_shared_expert(&self) -> bool {
        self.shared_dim > 0
    }

    /// Refuse a shape the engine cannot run, at construction rather than three layers into a
    /// forward pass.
    pub fn validate(&self) -> Result<(), ArtifactError> {
        // The recurrence's dimensions bind only where a linear layer exists: the qwen3moe
        // members are all-attention and carry zeros there.
        let linear_ok = if self.is_full_attention_only() {
            self.linear_k_heads == 0 && self.linear_v_heads == 0 && self.linear_head_dim == 0 && self.conv_kernel == 0
        } else {
            self.linear_k_heads > 0
                && self.linear_v_heads > 0
                && self.linear_v_heads.is_multiple_of(self.linear_k_heads)
                && self.linear_head_dim > 0
                && self.conv_kernel > 0
        };
        let ok = !self.layer_types.is_empty()
            && self.d_model > 0
            && self.n_heads > 0
            && self.n_kv_heads > 0
            && self.n_heads.is_multiple_of(self.n_kv_heads)
            && self.head_dim > 0
            && self.rotary_dim <= self.head_dim
            && self.rotary_dim.is_multiple_of(2)
            && linear_ok
            && self.n_experts > 0
            && self.experts_per_token > 0
            && self.experts_per_token <= self.n_experts
            && self.moe_dim > 0
            && self.vocab > 0
            && self.max_position > 0;
        if ok { Ok(()) } else { Err(ArtifactError::BadShape) }
    }
}

/// The artifact: a shape, a named weight store, a named parameter store, and the pinned rotary
/// table.
/// Where a tensor's codes live.
///
/// `Owned` is a fixture or a small class. `Mapped` is a 33 GiB file that does not fit in RAM and
/// is not supposed to: the mixture reads eight of 256 experts per token, so the resident set is a
/// fraction of the file. Which fraction, and who decides, is [`Qwen36ResidencyV1`]'s question
/// (ADR-0112). With a residency the forward pass never touches a mapped weight page: every byte
/// the arithmetic reads arrives through a read sized to its tensor, into memory this process owns
/// and counts. Without one the page cache decides — the behaviour before ADR-0112, which the
/// fleet measured at 11 MB/s through three million page faults a draw.
enum Store {
    Owned(BTreeMap<String, Vec<i8>>),
    Mapped { map: crate::mmap::ReadOnlyMap, directory: BTreeMap<String, (usize, usize)> },
}

/// A tensor's codes, wherever they are held: borrowed from an owned store or the mapping, or a
/// handle on bytes the residency holds. Derefs to the codes, so every kernel reads it as `&[i8]`.
/// The handle keeps its bytes alive for as long as a projection holds it, which is what lets the
/// residency evict an expert the moment its budget says so without any reader observing it.
pub enum TensorBytes<'a> {
    Borrowed(&'a [i8]),
    Held(std::sync::Arc<Vec<i8>>),
}

impl std::ops::Deref for TensorBytes<'_> {
    type Target = [i8];
    fn deref(&self) -> &[i8] {
        match self {
            Self::Borrowed(s) => s,
            Self::Held(v) => v.as_slice(),
        }
    }
}

impl std::fmt::Debug for TensorBytes<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let held = matches!(self, Self::Held(_));
        write!(f, "TensorBytes({} codes, {})", self.len(), if held { "held" } else { "borrowed" })
    }
}

impl PartialEq for TensorBytes<'_> {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

pub struct Qwen36ArtifactV1 {
    pub shape: Qwen36ShapeV1,
    store: Store,
    /// Parameter rows the process holds outright: every row of an owned store, and every
    /// non-expert row of a mapping. A budgeted mapping's expert rows ride with their expert
    /// (`Qwen36ResidencyV1`), read when it is admitted and given back when it is evicted.
    params: BTreeMap<String, Vec<u8>>,
    pub rope: RopeTableV1,
    /// ADR-0112: who decides which weights are in memory. `None` leaves it to the page cache.
    residency: Option<Qwen36ResidencyV1>,
}

/// **ADR-0112 Decision 2's ratio**: with no budget stated, a mapped class is held within a fifth
/// of its weight bytes.
pub const QWEN36_RESIDENT_FRACTION_DENOMINATOR_V1: u64 = 5;

/// **How much of a mapped class's weights this process keeps in memory** (ADR-0112 Decision 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Qwen36ResidencyPolicyV1 {
    /// No loader: weights are read through the mapping and the kernel's page cache decides —
    /// the behaviour before ADR-0112, kept so the two can be measured against each other.
    PageCache,
    /// Hold at most this many bytes of weights, the pinned always-set included.
    Bytes(u64),
    /// Hold at most a fifth of the artifact's weight bytes — the ratio ADR-0112 certifies.
    FifthOfTheWeights,
}

impl Qwen36ResidencyPolicyV1 {
    /// The budget in bytes for an artifact of `weight_bytes`, or `None` for the page cache.
    pub fn budget_for(self, weight_bytes: u64) -> Option<u64> {
        match self {
            Self::PageCache => None,
            Self::Bytes(b) => Some(b),
            Self::FifthOfTheWeights => Some(weight_bytes.div_ceil(QWEN36_RESIDENT_FRACTION_DENOMINATOR_V1)),
        }
    }
}

/// **The residency: which of a mapped class's weights this process holds, under a budget it
/// states** (ADR-0112 Decisions 1, 3 and 4).
///
/// The mixture reads eight of two hundred and fifty-six experts per layer per token, so 93 % of
/// the class is untouched on any given step. The page cache is an LRU over PAGES with no idea what
/// an expert is, and it reads a cold page through a mapping as one synchronous 4 KiB fault — on
/// the fleet's virtio disks 11 MB/s, against 845 MB/s for a read sized to a tensor, measured on
/// the same file the same day (ADR-0112 §1). So residency is decided here, in two tiers, because
/// there are two access patterns:
///
/// * **The always-set is pinned.** Every tensor that is neither a routed expert's nor the
///   embedding table — the norms, the recurrence's and attention's projections, the routers, the
///   shared experts, the unembedding: 1.86 GiB of the 33.27 GiB class — is read once at open,
///   through the file descriptor, into memory this process owns, and never given back. Every
///   token reads all of it.
/// * **Routed experts are an LRU** of owned buffers under what the budget leaves. A layer's eight
///   are read together, in parallel, the moment its router commits (`admit_experts`): one expert
///   is 3.09 MiB in three tensors and five parameter rows, and eight of them land in tens of
///   milliseconds where the page cache took the better part of a second. An expert read outside
///   an admission — a test, the inventory pass — is admitted on the way, so nothing depends on
///   the prefetch.
///
/// The embedding table (0.47 GiB) is neither: a token reads one row of it, which
/// [`Qwen36ArtifactV1::embedding_row`] reads directly.
///
/// The budget has a floor: the always-set plus one token's routed experts, the least a forward
/// pass can run in without re-reading what it just read. A budget below it is refused at open,
/// by name and with the numbers (`a_budget_below_the_floor_is_refused_by_name`).
///
/// # What it does NOT do
///
/// It does not change one bit of arithmetic. Residency is a decision about where bytes are, and
/// the class's whole claim is that the answer does not depend on that
/// (`a_budgeted_artifact_computes_what_an_owned_one_does_at_a_fifth_of_its_size`) — which is
/// also why the same policy is what a GPU tier would use, with the placement decision widened
/// from "resident or not" to "resident where".
pub struct Qwen36ResidencyV1 {
    /// The whole budget, pinned set included.
    budget: usize,
    pinned_bytes: usize,
    pinned: std::collections::HashMap<String, std::sync::Arc<Vec<i8>>>,
    /// Every routed expert's parts in the file — the directory a miss reads from.
    experts: std::collections::HashMap<(usize, usize), ExpertExtentsV1>,
    /// The expert parameter rows by name, in the artifact's canonical order — what the root pass
    /// absorbs, in the position an owned store's rows would have had.
    expert_param_extents: BTreeMap<String, (usize, usize)>,
    /// The widest expert, in bytes.
    expert_bytes: usize,
    /// How many layers carry routed experts, and how many a token routes to in each.
    expert_layers: usize,
    experts_per_token: usize,
    inner: std::sync::Mutex<ResidencyInnerV1>,
}

/// One routed expert's parts in the file: `(name, offset, len)` per weight tensor (the codes and,
/// where the class carries them, the group exponents) and per parameter row.
#[derive(Default)]
struct ExpertExtentsV1 {
    tensors: Vec<(String, usize, usize)>,
    params: Vec<(String, usize, usize)>,
    bytes: usize,
}

/// One routed expert, held.
struct HeldExpertV1 {
    tensors: Vec<(String, std::sync::Arc<Vec<i8>>)>,
    params: Vec<(String, std::sync::Arc<Vec<u8>>)>,
    bytes: usize,
}

struct ResidencyInnerV1 {
    held: std::collections::HashMap<(usize, usize), HeldExpertV1>,
    /// Most recently used last.
    order: std::collections::VecDeque<(usize, usize)>,
    bytes: usize,
    hits: u64,
    misses: u64,
    evictions: u64,
    bytes_read: u64,
}

/// The residency's numbers, for a log line or a test (ADR-0112 Decision 8).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Qwen36ResidencyStatsV1 {
    pub budget_bytes: u64,
    pub pinned_bytes: u64,
    /// The routed experts held right now, in bytes — never past `budget_bytes − pinned_bytes`
    /// between admissions.
    pub resident_expert_bytes: u64,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    /// Bytes this loader read from the file since open: the pinned set once, then every miss.
    pub bytes_read: u64,
    /// One token's routed experts, in bytes: what the expert budget is counted in tokens of.
    pub token_expert_bytes: u64,
}

impl Qwen36ResidencyStatsV1 {
    /// What the budget leaves for routed experts.
    pub fn expert_budget_bytes(&self) -> u64 {
        self.budget_bytes.saturating_sub(self.pinned_bytes)
    }
}

/// `blk.{layer}.ffn_expert.{expert}_{role}` → `((layer, expert), role)`. The one name-shaped
/// rule the artifact format has (the writer's naming), applied here for tensors and parameter
/// rows alike. The shared expert is `ffn_shared_expert_…` and does not match, which is right: it
/// runs for every token and is pinned.
fn expert_key_v1(name: &str) -> Option<((usize, usize), &str)> {
    let rest = name.strip_prefix("blk.")?;
    let (layer, rest) = rest.split_once(".ffn_expert.")?;
    let (expert, role) = rest.split_once('_')?;
    Some(((layer.parse().ok()?, expert.parse().ok()?), role))
}

impl Qwen36ResidencyV1 {
    fn stats(&self) -> Qwen36ResidencyStatsV1 {
        let inner = self.inner.lock().expect("the residency lock is never poisoned");
        Qwen36ResidencyStatsV1 {
            budget_bytes: self.budget as u64,
            pinned_bytes: self.pinned_bytes as u64,
            resident_expert_bytes: inner.bytes as u64,
            hits: inner.hits,
            misses: inner.misses,
            evictions: inner.evictions,
            bytes_read: inner.bytes_read,
            token_expert_bytes: self.one_token_expert_bytes() as u64,
        }
    }

    fn expert_budget(&self) -> usize {
        self.budget.saturating_sub(self.pinned_bytes)
    }

    /// The least budget this class's forward pass runs in: the always-set, and one token's
    /// routed experts across every layer that has them.
    fn floor(&self) -> usize {
        self.pinned_bytes.saturating_add(self.one_token_expert_bytes())
    }

    fn one_token_expert_bytes(&self) -> usize {
        self.expert_bytes.saturating_mul(self.experts_per_token).saturating_mul(self.expert_layers)
    }

    /// Read one expert's parts through the file descriptor: the read a miss costs.
    fn read_expert(map: &crate::mmap::ReadOnlyMap, extents: &ExpertExtentsV1) -> Result<HeldExpertV1, Qwen36Error> {
        let unreadable = |name: &str, e: std::io::Error| Qwen36Error::Unreadable(format!("{name}: {e}"));
        let mut tensors = Vec::with_capacity(extents.tensors.len());
        for (name, offset, len) in &extents.tensors {
            tensors.push((name.clone(), std::sync::Arc::new(map.read_i8_at(*offset, *len).map_err(|e| unreadable(name, e))?)));
        }
        let mut params = Vec::with_capacity(extents.params.len());
        for (name, offset, len) in &extents.params {
            params.push((name.clone(), std::sync::Arc::new(map.read_u8_at(*offset, *len).map_err(|e| unreadable(name, e))?)));
        }
        Ok(HeldExpertV1 { tensors, params, bytes: extents.bytes })
    }

    /// Put a read expert in, most recently used, and give back the coldest until the budget
    /// holds. An expert held by a projection in flight stays alive through its handle; only the
    /// residency's count drops, which is the transient the floor is sized for.
    fn insert_locked(&self, inner: &mut ResidencyInnerV1, key: (usize, usize), held: HeldExpertV1) {
        inner.bytes_read += held.bytes as u64;
        if let Some(previous) = inner.held.insert(key, held) {
            // Two threads missed the same expert at once; one read is kept, the other's bytes
            // were read for nothing and are not counted twice as resident.
            inner.bytes = inner.bytes.saturating_sub(previous.bytes);
            inner.order.retain(|k| *k != key);
        }
        inner.bytes += inner.held[&key].bytes;
        inner.order.push_back(key);
        let budget = self.expert_budget();
        while inner.bytes > budget {
            let Some(cold) = inner.order.pop_front() else { break };
            if let Some(gone) = inner.held.remove(&cold) {
                inner.bytes = inner.bytes.saturating_sub(gone.bytes);
                inner.evictions += 1;
            }
        }
    }

    fn touch_locked(inner: &mut ResidencyInnerV1, key: (usize, usize)) {
        if let Some(i) = inner.order.iter().position(|k| *k == key) {
            inner.order.remove(i);
            inner.order.push_back(key);
        }
    }

    /// **The prefetch** (ADR-0112 Decision 4): every chosen expert of `layer` not yet held, read
    /// together and in parallel, before any of them is computed.
    fn admit(&self, map: &crate::mmap::ReadOnlyMap, layer: usize, chosen: &[usize]) {
        let missing: Vec<(usize, usize)> = {
            let mut inner = self.inner.lock().expect("the residency lock is never poisoned");
            let mut missing = Vec::new();
            for expert in chosen {
                let key = (layer, *expert);
                if inner.held.contains_key(&key) {
                    inner.hits += 1;
                    Self::touch_locked(&mut inner, key);
                } else if self.experts.contains_key(&key) && !missing.contains(&key) {
                    inner.misses += 1;
                    missing.push(key);
                }
            }
            missing
        };
        if missing.is_empty() {
            return;
        }
        // Off the lock: the reads are the slow part, and another duty's forward pass on the same
        // artifact must not wait behind them. A read that fails here is not an error yet — the
        // projection that needs the expert reads it again and reports the failure by name.
        use rayon::prelude::*;
        let read: Vec<((usize, usize), HeldExpertV1)> =
            missing.par_iter().filter_map(|key| Self::read_expert(map, &self.experts[key]).ok().map(|held| (*key, held))).collect();
        let mut inner = self.inner.lock().expect("the residency lock is never poisoned");
        for (key, held) in read {
            self.insert_locked(&mut inner, key, held);
        }
    }

    /// One part of one expert — a hit, or a miss admitted on the way.
    fn expert_part<T>(
        &self,
        map: &crate::mmap::ReadOnlyMap,
        key: (usize, usize),
        name: &str,
        part: impl Fn(&HeldExpertV1) -> Option<T>,
    ) -> Result<T, Qwen36Error> {
        let extents = self.experts.get(&key).ok_or_else(|| Qwen36Error::MissingTensor(name.to_string()))?;
        {
            let mut inner = self.inner.lock().expect("the residency lock is never poisoned");
            if let Some(held) = inner.held.get(&key) {
                let found = part(held);
                inner.hits += 1;
                Self::touch_locked(&mut inner, key);
                return found.ok_or_else(|| Qwen36Error::MissingTensor(name.to_string()));
            }
            inner.misses += 1;
        }
        let held = Self::read_expert(map, extents)?;
        let found = part(&held);
        let mut inner = self.inner.lock().expect("the residency lock is never poisoned");
        self.insert_locked(&mut inner, key, held);
        found.ok_or_else(|| Qwen36Error::MissingTensor(name.to_string()))
    }

    fn expert_tensor(
        &self,
        map: &crate::mmap::ReadOnlyMap,
        key: (usize, usize),
        name: &str,
    ) -> Result<std::sync::Arc<Vec<i8>>, Qwen36Error> {
        self.expert_part(map, key, name, |held| held.tensors.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone()))
    }

    fn expert_param(
        &self,
        map: &crate::mmap::ReadOnlyMap,
        key: (usize, usize),
        name: &str,
    ) -> Result<std::sync::Arc<Vec<u8>>, Qwen36Error> {
        self.expert_part(map, key, name, |held| held.params.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone()))
    }
}

/// Why the engine refused. Every one is a REGISTRATION defect surfaced before the pass starts,
/// except `Position`, which is a caller error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Qwen36Error {
    MissingTensor(String),
    MissingParams(String),
    BadTensor {
        name: String,
        want: usize,
        got: usize,
    },
    BadParams(String),
    OpRefused(&'static str, String),
    Position,
    /// ADR-0112: a residency budget this class cannot run in, with the floor and its terms.
    Residency(String),
    /// The file could not be read where the directory said a tensor or a row was.
    Unreadable(String),
}

impl std::fmt::Display for Qwen36Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingTensor(n) => write!(f, "the artifact has no tensor {n}"),
            Self::MissingParams(n) => write!(f, "the artifact has no parameter row {n}"),
            Self::BadTensor { name, want, got } => write!(f, "tensor {name} should hold {want} values and holds {got}"),
            Self::BadParams(n) => write!(f, "parameter row {n} is malformed"),
            Self::OpRefused(w, why) => write!(f, "the op {w} refused its input: {why}"),
            Self::Position => write!(f, "the position is past the rotary table"),
            Self::Residency(why) => write!(f, "{why}"),
            Self::Unreadable(why) => write!(f, "the artifact is unreadable: {why}"),
        }
    }
}

impl std::error::Error for Qwen36Error {}

impl Qwen36ArtifactV1 {
    pub fn new(shape: Qwen36ShapeV1, rope: RopeTableV1) -> Result<Self, ArtifactError> {
        shape.validate()?;
        Ok(Self { shape, store: Store::Owned(BTreeMap::new()), params: BTreeMap::new(), rope, residency: None })
    }

    pub fn with_tensor(mut self, name: impl Into<String>, values: Vec<i8>) -> Self {
        match &mut self.store {
            Store::Owned(t) => {
                t.insert(name.into(), values);
            }
            Store::Mapped { .. } => panic!("a mapped artifact is read-only; build it with the writer"),
        }
        self
    }

    pub fn with_params(mut self, name: impl Into<String>, rows: &[A16QuantParams]) -> Self {
        self.params.insert(name.into(), rows.iter().flat_map(|p| p.to_wire()).collect());
        self
    }

    /// A tensor's codes: from the owned store, from the residency (a pinned tensor's handle, or
    /// a routed expert's — admitted on the way if it is not held), or, with no residency or for
    /// the embedding table, a slice of the mapping.
    pub fn tensor(&self, name: &str) -> Result<TensorBytes<'_>, Qwen36Error> {
        match &self.store {
            Store::Owned(t) => {
                t.get(name).map(|v| TensorBytes::Borrowed(&v[..])).ok_or_else(|| Qwen36Error::MissingTensor(name.to_string()))
            }
            Store::Mapped { map, directory } => {
                if let Some(residency) = &self.residency {
                    if let Some((key, _)) = expert_key_v1(name) {
                        return residency.expert_tensor(map, key, name).map(TensorBytes::Held);
                    }
                    if let Some(held) = residency.pinned.get(name) {
                        return Ok(TensorBytes::Held(held.clone()));
                    }
                    // The embedding table is neither pinned nor an expert: a token reads one row
                    // of it (`embedding_row`), and a caller asking for the whole table — a test,
                    // the inventory — reads it through the mapping.
                }
                let (offset, len) = *directory.get(name).ok_or_else(|| Qwen36Error::MissingTensor(name.to_string()))?;
                // A directory entry that leaves the mapping is a truncated file, which is a
                // refusal rather than a fault — the bytes are data a producer was handed.
                map.i8_slice(offset, len).map(TensorBytes::Borrowed).ok_or_else(|| Qwen36Error::BadTensor {
                    name: name.to_string(),
                    want: len,
                    got: 0,
                })
            }
        }
    }

    /// **A tensor's byte length, without touching one of its bytes** (ADR-0106) — present exactly
    /// when [`Self::tensor`] would answer: an absent name and an extent that leaves the mapping are
    /// the same refusals here.
    pub fn tensor_len(&self, name: &str) -> Result<usize, Qwen36Error> {
        match &self.store {
            Store::Owned(t) => t.get(name).map(|v| v.len()).ok_or_else(|| Qwen36Error::MissingTensor(name.to_string())),
            Store::Mapped { map, directory } => {
                let (offset, len) = *directory.get(name).ok_or_else(|| Qwen36Error::MissingTensor(name.to_string()))?;
                match offset.checked_add(len) {
                    Some(end) if end <= map.len() => Ok(len),
                    _ => Err(Qwen36Error::BadTensor { name: name.to_string(), want: len, got: 0 }),
                }
            }
        }
    }

    /// **`buf.len()` bytes of a tensor from byte `at`, copied into the caller's buffer** (ADR-0106)
    /// — a mapped store reads them through the file descriptor, never the mapping, for the reason
    /// [`Self::artifact_root`] gives: a whole-artifact pass that faults the map runs at the fault
    /// rate and leaves every page it touched resident, and one that reads leaves only its buffer.
    /// A range past the tensor is a refusal; an IO error names the byte it failed at.
    pub fn read_tensor_range_into(&self, name: &str, at: usize, buf: &mut [u8]) -> Result<(), String> {
        let len = self.tensor_len(name).map_err(|e| e.to_string())?;
        if at.checked_add(buf.len()).is_none_or(|end| end > len) {
            return Err(format!("{name}: bytes {at}..+{} leave its {len}", buf.len()));
        }
        match &self.store {
            Store::Owned(t) => {
                let codes = &t[name][at..at + buf.len()];
                for (out, v) in buf.iter_mut().zip(codes) {
                    *out = *v as u8;
                }
                Ok(())
            }
            Store::Mapped { map, directory } => {
                let (offset, _) = directory[name];
                let from = (offset + at) as u64;
                map.read_exact_at(from, buf).map_err(|e| format!("{name}: unreadable at file byte {from}: {e}"))
            }
        }
    }

    /// **One token's embedding row** — `d` codes at row `token_id` of `token_embd.weight`. Under
    /// a residency the row is read directly (2 KiB through the file descriptor), so the 0.47 GiB
    /// table is neither pinned nor faulted; otherwise it is a slice of the table.
    pub fn embedding_row(&self, token_id: usize, d: usize) -> Result<Vec<i8>, Qwen36Error> {
        const TABLE: &str = "token_embd.weight";
        let want = self.shape.vocab.saturating_mul(d);
        if let (Store::Mapped { map, directory }, Some(_)) = (&self.store, &self.residency) {
            let (offset, len) = *directory.get(TABLE).ok_or_else(|| Qwen36Error::MissingTensor(TABLE.to_string()))?;
            if len != want {
                return Err(Qwen36Error::BadTensor { name: TABLE.to_string(), want, got: len });
            }
            let at = offset.checked_add(token_id.saturating_mul(d)).ok_or(Qwen36Error::Position)?;
            return map.read_i8_at(at, d).map_err(|e| Qwen36Error::Unreadable(format!("{TABLE}: {e}")));
        }
        let table = self.tensor_sized(TABLE, want)?;
        Ok(table[token_id * d..(token_id + 1) * d].to_vec())
    }

    /// **Hand the residency every routed expert `chosen` of `layer`, before any of them is
    /// computed** (ADR-0112 Decision 4). Called with the routing the moment it commits, so the
    /// eight reads are in flight together rather than one at a time behind the arithmetic that
    /// consumes them. Nothing without a residency; changes no bit of arithmetic with one.
    pub fn admit_experts(&self, layer: usize, chosen: &[usize]) {
        if let (Store::Mapped { map, .. }, Some(residency)) = (&self.store, &self.residency) {
            residency.admit(map, layer, chosen);
        }
    }

    /// The residency's numbers, or `None` when the page cache decides.
    pub fn residency_stats(&self) -> Option<Qwen36ResidencyStatsV1> {
        self.residency.as_ref().map(Qwen36ResidencyV1::stats)
    }

    /// The least residency budget this artifact's forward pass runs in — its always-set plus one
    /// token's routed experts — or `None` for an owned store, which holds everything anyway.
    pub fn residency_floor_bytes(&self) -> Option<u64> {
        self.residency.as_ref().map(|r| r.floor() as u64)
    }

    /// **The artifact's identity: one digest over everything a forward pass reads.**
    ///
    /// The shape, every parameter table, the rotary table and every weight byte, each under a
    /// length-prefixed name so two different directories cannot collide by concatenation. This is
    /// what a `ClassRegistered` carries as `artifact_root` and what a producer proves it holds —
    /// a node that computes a different digest is holding different weights, whatever the file is
    /// called.
    ///
    /// One pass over the mapping (~10 s warm for the 33 GiB class, about a minute cold), computed
    /// at startup rather than per block. The tokenizer is deliberately NOT inside: this class
    /// binds prompts by token-id hash, so tokenization is outside the computation a court
    /// reproduces (unlike the dense tier, whose artifact carries a tokenizer commitment).
    pub fn artifact_root(&self) -> Hash64 {
        const DOMAIN: &[u8] = b"misaka-palw/qwen36/artifact-root/v1";
        let mut state = blake2b_simd::Params::new().hash_length(64).key(DOMAIN).to_state();
        let absorb = |state: &mut blake2b_simd::State, tag: &[u8], name: &str, bytes: &[u8]| {
            state.update(&(tag.len() as u64).to_le_bytes());
            state.update(tag);
            state.update(&(name.len() as u64).to_le_bytes());
            state.update(name.as_bytes());
            state.update(&(bytes.len() as u64).to_le_bytes());
            state.update(bytes);
        };
        // The shape, field by field in declaration order — the same bytes `qwen36_shape_id_v1`
        // reads, but inside this digest rather than beside it.
        let mut shape = Vec::with_capacity(24 * 8 + self.shape.layer_types.len());
        for kind in &self.shape.layer_types {
            shape.push(match kind {
                Qwen36LayerKind::LinearAttention => 0u8,
                Qwen36LayerKind::FullAttention => 1u8,
            });
        }
        for v in [
            self.shape.d_model,
            self.shape.n_heads,
            self.shape.n_kv_heads,
            self.shape.head_dim,
            self.shape.rotary_dim,
            self.shape.linear_k_heads,
            self.shape.linear_v_heads,
            self.shape.linear_head_dim,
            self.shape.conv_kernel,
            self.shape.n_experts,
            self.shape.experts_per_token,
            self.shape.moe_dim,
            self.shape.shared_dim,
            self.shape.vocab,
            self.shape.max_position,
        ] {
            shape.extend_from_slice(&(v as u64).to_le_bytes());
        }
        shape.extend_from_slice(&self.shape.eps_q.to_le_bytes());
        shape.push(self.shape.router_up_bits);
        absorb(&mut state, b"shape", "", &shape);
        let mut rope = Vec::with_capacity((self.rope.cos_q.len() + self.rope.sin_q.len()) * 4 + 16);
        rope.extend_from_slice(&(self.rope.d_head as u64).to_le_bytes());
        rope.extend_from_slice(&(self.rope.max_position as u64).to_le_bytes());
        for v in self.rope.cos_q.iter().chain(&self.rope.sin_q) {
            rope.extend_from_slice(&v.to_le_bytes());
        }
        absorb(&mut state, b"rope", "", &rope);
        // BTreeMaps, so both stores absorb in one canonical order — and a budgeted mapping's
        // expert rows, which live in the file rather than in `params`, take the place in that
        // order an owned store's rows would have had: two sorted lists merged by name.
        let expert_rows = self.residency.as_ref().map(|r| &r.expert_param_extents);
        let mut owned = self.params.iter().peekable();
        let mut extents = expert_rows.map(|m| m.iter()).into_iter().flatten().peekable();
        let mut row = vec![0u8; 0];
        loop {
            let take_owned = match (owned.peek(), extents.peek()) {
                (None, None) => break,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (Some((a, _)), Some((b, _))) => a <= b,
            };
            if take_owned {
                let (name, bytes) = owned.next().expect("peeked");
                absorb(&mut state, b"param", name, bytes);
            } else {
                let (name, (offset, len)) = extents.next().expect("peeked");
                let Store::Mapped { map, .. } = &self.store else { unreachable!("expert extents exist only for a mapping") };
                row.resize(*len, 0);
                map.read_exact_at(*offset as u64, &mut row).unwrap_or_else(|e| {
                    panic!("the artifact became unreadable at parameter row {name} while computing its root: {e}")
                });
                absorb(&mut state, b"param", name, &row);
            }
        }
        match &self.store {
            Store::Owned(tensors) => {
                for (name, codes) in tensors {
                    // SAFETY-free reinterpret: i8 and u8 share a layout.
                    let bytes: &[u8] = unsafe { std::slice::from_raw_parts(codes.as_ptr() as *const u8, codes.len()) };
                    absorb(&mut state, b"tensor", name, bytes);
                }
            }
            Store::Mapped { map, directory } => {
                // The pass reads nearly the whole file once, and it deliberately does NOT read it
                // through the mapping: a cold page through the map is a synchronous 4 KiB fault,
                // and on the fleet's own virtio disks fault readahead never engages — not under
                // MADV_SEQUENTIAL, not under MADV_WILLNEED, not with the device's readahead
                // window raised, all three measured on the day this line was written, at 6 MB/s
                // against a device that streams at 1.3 GB/s. So the pass streams each tensor
                // through `read_exact_at` in large chunks instead — same inode, same page cache,
                // same bytes, and the hash is a stream, so absorbing a tensor in chunks is the
                // same state as absorbing it whole. The length prefix keeps the old semantics
                // exactly: an extent outside the map absorbs as empty, never as a short read.
                let mut buf = vec![0u8; 32 << 20];
                for (name, (offset, len)) in directory {
                    let in_range = offset.checked_add(*len).is_some_and(|end| end <= map.as_bytes().len());
                    let absorbed = if in_range { *len } else { 0 };
                    state.update(&(b"tensor".len() as u64).to_le_bytes());
                    state.update(b"tensor");
                    state.update(&(name.len() as u64).to_le_bytes());
                    state.update(name.as_bytes());
                    state.update(&(absorbed as u64).to_le_bytes());
                    let mut at = *offset as u64;
                    let mut remaining = absorbed;
                    while remaining > 0 {
                        let take = remaining.min(buf.len());
                        // An IO error mid-pass is the mapped path's SIGBUS with a name on it: the
                        // artifact is unreadable and no root this node could report is true.
                        map.read_exact_at(at, &mut buf[..take])
                            .unwrap_or_else(|e| panic!("the artifact became unreadable at byte {at} while computing its root: {e}"));
                        state.update(&buf[..take]);
                        at += take as u64;
                        remaining -= take;
                    }
                }
            }
        }
        let mut out = [0u8; 64];
        out.copy_from_slice(state.finalize().as_bytes());
        Hash64::from_bytes(out)
    }

    /// Every tensor name the artifact holds, in order.
    pub fn tensor_names(&self) -> Vec<&str> {
        match &self.store {
            Store::Owned(t) => t.keys().map(|k| k.as_str()).collect(),
            Store::Mapped { directory, .. } => directory.keys().map(|k| k.as_str()).collect(),
        }
    }

    /// Total bytes of weight codes.
    pub fn weight_bytes(&self) -> usize {
        match &self.store {
            Store::Owned(t) => t.values().map(|v| v.len()).sum(),
            Store::Mapped { directory, .. } => directory.values().map(|(_, n)| *n).sum(),
        }
    }

    pub(crate) fn tensor_sized(&self, name: &str, want: usize) -> Result<TensorBytes<'_>, Qwen36Error> {
        let row = self.tensor(name)?;
        if row.len() != want {
            return Err(Qwen36Error::BadTensor { name: name.to_string(), want, got: row.len() });
        }
        Ok(row)
    }

    /// One parameter row, decoded. Widths are checked by the caller that knows what it asked for.
    /// A budgeted mapping's expert rows come from the expert's holding (admitted on the way, like
    /// its tensors); every other row is the process's own.
    pub fn param_rows(&self, name: &str) -> Result<Vec<A16QuantParams>, Qwen36Error> {
        let held;
        let bytes: &[u8] = match (&self.store, &self.residency, expert_key_v1(name)) {
            (Store::Mapped { map, .. }, Some(residency), Some((key, _))) if residency.experts.contains_key(&key) => {
                held = residency.expert_param(map, key, name).map_err(|_| Qwen36Error::MissingParams(name.to_string()))?;
                held.as_slice()
            }
            _ => self.params.get(name).ok_or_else(|| Qwen36Error::MissingParams(name.to_string()))?,
        };
        if bytes.is_empty() || !bytes.len().is_multiple_of(A16QuantParams::WIRE_BYTES) {
            return Err(Qwen36Error::BadParams(name.to_string()));
        }
        bytes
            .chunks_exact(A16QuantParams::WIRE_BYTES)
            .map(|c| A16QuantParams::from_wire(c).map_err(|_| Qwen36Error::BadParams(name.to_string())))
            .collect()
    }

    pub(crate) fn one_param(&self, name: &str) -> Result<A16QuantParams, Qwen36Error> {
        let rows = self.param_rows(name)?;
        if rows.len() != 1 {
            return Err(Qwen36Error::BadParams(name.to_string()));
        }
        Ok(rows[0])
    }

    pub(crate) fn params_sized(&self, name: &str, want: usize) -> Result<Vec<A16QuantParams>, Qwen36Error> {
        let rows = self.param_rows(name)?;
        // A per-layer-uniform triple is stored once and tiled, which keeps the store small without
        // a second layout — the same rule the dense tier's oracle applies.
        if rows.len() == want {
            return Ok(rows);
        }
        if rows.len() == 1 {
            return Ok(vec![rows[0]; want]);
        }
        Err(Qwen36Error::BadParams(name.to_string()))
    }

    /// A group-scale exponent table for a weight tensor, if the class registered one.
    ///
    /// Absent means the weights carry one scale per output row, which is what a fixture and the
    /// first artifacts hold. Present means a power-of-two scale per 32 elements.
    pub fn group_exponents(&self, name: &str) -> Option<TensorBytes<'_>> {
        self.tensor(&format!("{name}.exp")).ok()
    }

    /// A registered scalar, in Q[`K`]. Carried in a triple's `zero` so the store has one wire
    /// format rather than two.
    pub(crate) fn scalar(&self, name: &str) -> Result<i64, Qwen36Error> {
        Ok(self.one_param(name)?.zero)
    }
}

/// Every op in this module returns its own error type; the engine returns one. Written as a free
/// generic rather than a closure per function because a closure's error type is inferred from its
/// first use and the arms mix `PalwA16OpError` with `PalwQwen36OpError`.
/// A projection's error label. `refuse` wants a `&'static str` and a tensor name is not one, so
/// the site is named by its kind and the tensor appears in the message the op itself produced.
fn name_leak(_name: &str) -> &'static str {
    "projection"
}

fn refuse<E: std::fmt::Debug>(what: &'static str) -> impl Fn(E) -> Qwen36Error {
    // The inner error is kept. A runtime whose only diagnostic is "an op refused" makes every
    // conversion bug a bisection over the graph, which is exactly what happened the first time
    // this ran on real weights.
    move |e| Qwen36Error::OpRefused(what, format!("{e:?}"))
}

/// The runtime state one sequence carries.
///
/// A GatedDeltaNet layer's state is a `d_v × d_k` matrix per value head and it is the whole
/// history — there is no growing cache. A full-attention layer keeps the usual keys and values.
/// Both live here so that a caller holds one object per sequence.
pub struct Qwen36Cache {
    /// Per layer (empty for full-attention layers): one state per value head.
    pub gdn: Vec<Vec<Qwen36GdnStateV1>>,
    /// Per layer (empty for full-attention layers): the convolution's `kernel` most recent rows,
    /// oldest first, over the concatenated q/k/v channels.
    pub conv: Vec<Vec<Vec<i32>>>,
    /// Per layer (empty for linear-attention layers).
    pub keys: Vec<Vec<Vec<i32>>>,
    pub values: Vec<Vec<Vec<i32>>>,
}

impl Qwen36Cache {
    pub fn new(shape: &Qwen36ShapeV1) -> Self {
        let n = shape.n_layers();
        let mut gdn = vec![Vec::new(); n];
        let mut conv = vec![Vec::new(); n];
        let conv_width = 2 * shape.linear_k_dim() + shape.linear_v_dim();
        for (li, kind) in shape.layer_types.iter().enumerate() {
            if *kind == Qwen36LayerKind::LinearAttention {
                gdn[li] =
                    (0..shape.linear_v_heads).map(|_| Qwen36GdnStateV1::zeros(shape.linear_head_dim, shape.linear_head_dim)).collect();
                conv[li] = vec![vec![0; conv_width]; shape.conv_kernel];
            }
        }
        Self { gdn, conv, keys: vec![Vec::new(); n], values: vec![Vec::new(); n] }
    }

    /// How many positions the softmax layers have seen. Zero for a fresh cache.
    pub fn len(&self) -> usize {
        self.keys.iter().map(|k| k.len()).max().unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// One probe entry: a site's name and the committed lanes at it.
///
/// A named type because the alternative reads as three levels of tuple at every signature.
pub type Qwen36ProbeV1 = Vec<(String, Vec<i32>)>;

/// One probe entry reduced to a peak.
pub type Qwen36PeaksV1 = Vec<(String, i32)>;

/// The engine. Residency is the artifact's (ADR-0112), so every engine over one artifact — a
/// producer's, a seat's, a court's, in one process — shares one set of held weights.
pub struct Qwen36Engine<'a> {
    pub artifact: &'a Qwen36ArtifactV1,
}

impl<'a> Qwen36Engine<'a> {
    /// **One projection, through whichever weight representation the class registered.**
    ///
    /// Every projection in this graph goes through here so that adding a finer weight scale is one
    /// decision rather than eighteen. When the artifact carries `<name>.exp` the weights hold a
    /// power-of-two scale per 32 elements — Q4_K's granularity, and measured at 5.3e-3 relative
    /// against 8.7e-3 for one scale per row — and otherwise the plain per-row form is used, which
    /// is what a fixture and the older artifacts carry.
    pub(crate) fn project(&self, name: &str, x: &[i32], out_dim: usize, wide: bool) -> Result<Vec<i32>, Qwen36Error> {
        let a = self.artifact;
        let weights = a.tensor_sized(name, out_dim * x.len())?;
        let params = a.params_sized(&format!("{name}.a16"), out_dim)?;
        match a.group_exponents(name) {
            Some(exps) if wide => {
                crate::kernels::q36_matmul_grouped_wide_fast(&weights, &exps, x, &params).map_err(refuse(name_leak(name)))
            }
            Some(exps) => crate::kernels::q36_matmul_grouped_fast(&weights, &exps, x, &params).map_err(refuse(name_leak(name))),
            None if wide => a16_matmul_rescale(&weights, x, &params).map_err(refuse(name_leak(name))),
            None => a16_matmul_requant(&weights, x, &params).map_err(refuse(name_leak(name))),
        }
    }

    pub fn new(artifact: &'a Qwen36ArtifactV1) -> Self {
        Self { artifact }
    }

    /// One position. Returns the committed logit row: i16 codes in i32 lanes, argmax over which
    /// breaks ties to the lowest id.
    pub fn forward_token(&self, cache: &mut Qwen36Cache, token_id: usize, position: usize) -> Result<Vec<i32>, Qwen36Error> {
        self.forward_token_probed(cache, token_id, position).map(|(logits, _)| logits)
    }

    /// The probe reduced to one peak per site.
    pub fn forward_token_peaks(
        &self,
        cache: &mut Qwen36Cache,
        token_id: usize,
        position: usize,
    ) -> Result<(Vec<i32>, Qwen36PeaksV1), Qwen36Error> {
        let (logits, rows) = self.forward_token_probed(cache, token_id, position)?;
        let peaks = rows.into_iter().map(|(n, r)| (n, r.iter().map(|v| v.abs()).max().unwrap_or(0))).collect();
        Ok((logits, peaks))
    }

    /// The same pass, plus the residual stream after every arm and every mixture.
    ///
    /// A quantized graph that runs and says nothing sensible is a graph with a scale error, and a
    /// scale error is invisible in the logits — it looks like a different model. What makes it
    /// findable is comparing the stream's MAGNITUDE against the reference's at the same site: the
    /// stage where the ratio stops being one is the stage that is wrong.
    pub fn forward_token_probed(
        &self,
        cache: &mut Qwen36Cache,
        token_id: usize,
        position: usize,
    ) -> Result<(Vec<i32>, Qwen36ProbeV1), Qwen36Error> {
        let a = self.artifact;
        let s = &a.shape;
        let d = s.d_model;
        if token_id >= s.vocab {
            return Err(Qwen36Error::Position);
        }

        let row: Vec<i32> = a.embedding_row(token_id, d)?.iter().map(|c| *c as i32).collect();
        // **The lift is per TOKEN, not per class.** One scale for a 248,320-row embedding table is
        // one scale for its outliers, and a prompt's ordinary rows then land on a fraction of the
        // int8 range — the resolution the whole forward pass starts from. A store with one row is
        // still read (a fixture has one); a store with `vocab` rows is indexed by the token.
        let lift = a.param_rows("embed_lift.a16")?;
        let p = if lift.len() == 1 {
            lift[0]
        } else {
            *lift.get(token_id).ok_or_else(|| Qwen36Error::BadParams("embed_lift.a16 is shorter than the vocabulary".into()))?
        };
        let mut h = a16_requant(&row, &vec![p; d]).map_err(refuse("embed_lift"))?;
        let mut probe: Vec<(String, Vec<i32>)> = vec![("embed".to_string(), h.clone())];

        for li in 0..s.n_layers() {
            let n = |suffix: &str| format!("blk.{li}.{suffix}");
            // ---- the arm ------------------------------------------------------------------
            let unit = a16_rms_norm(&h, s.eps_q).map_err(refuse("attn_norm"))?;
            let normed = a16_requant(&unit, &a.params_sized(&n("attn_norm.a16"), d)?).map_err(refuse("attn_norm_req"))?;
            probe.push((n("attn_norm"), normed.clone()));
            let delta = match s.layer_types[li] {
                Qwen36LayerKind::LinearAttention => self.linear_arm(cache, li, &normed, &mut probe)?,
                Qwen36LayerKind::FullAttention => self.full_arm(cache, li, &normed, position, &mut probe)?,
            };
            probe.push((n("linear_out"), delta.clone()));
            let aligned = a16_requant(&h, &a.params_sized(&n("attn_align.a16"), d)?).map_err(refuse("attn_align"))?;
            let sum = a16_add_elem(&aligned, &delta).map_err(refuse("attn_add"))?;
            h = a16_requant(&sum, &a.params_sized(&n("attn_residual.a16"), d)?).map_err(refuse("attn_res"))?;
            probe.push((n("attn_residual"), h.clone()));

            // ---- the mixture ---------------------------------------------------------------
            let unit = a16_rms_norm(&h, s.eps_q).map_err(refuse("ffn_norm"))?;
            let normed = a16_requant(&unit, &a.params_sized(&n("ffn_norm.a16"), d)?).map_err(refuse("ffn_norm_req"))?;
            probe.push((n("ffn_norm"), normed.clone()));
            let delta = self.moe(li, &normed, &mut probe)?;
            let aligned = a16_requant(&h, &a.params_sized(&n("ffn_align.a16"), d)?).map_err(refuse("ffn_align"))?;
            let sum = a16_add_elem(&aligned, &delta).map_err(refuse("ffn_add"))?;
            h = a16_requant(&sum, &a.params_sized(&n("ffn_residual.a16"), d)?).map_err(refuse("ffn_res"))?;
            probe.push((n("ffn_residual"), h.clone()));
        }

        let unit = a16_rms_norm(&h, s.eps_q).map_err(refuse("final_norm"))?;
        let fin = a16_requant(&unit, &a.params_sized("final_norm.a16", d)?).map_err(refuse("final_req"))?;
        probe.push(("final_norm".to_string(), fin.clone()));
        let logits = self.project("output.weight", &fin, s.vocab, false)?;
        probe.push(("logits".to_string(), logits.clone()));
        Ok((logits, probe))
    }

    /// **The GatedDeltaNet arm.** Convolve, normalize the key, run the recurrence, gate the
    /// output, project out. Four nodes plus the projections, none of them fused.
    fn linear_arm(
        &self,
        cache: &mut Qwen36Cache,
        li: usize,
        normed: &[i32],
        probe: &mut Vec<(String, Vec<i32>)>,
    ) -> Result<Vec<i32>, Qwen36Error> {
        let a = self.artifact;
        let s = &a.shape;
        let (_d, dk, dv, hd) = (s.d_model, s.linear_k_dim(), s.linear_v_dim(), s.linear_head_dim);
        let n = |suffix: &str| format!("blk.{li}.{suffix}");

        // The three projections that feed the convolution, plus the gate and the two scalars the
        // decay needs. Separate tensors rather than one fused `in_proj`: a court opening addresses
        // a tensor, and a fused one would need an offset convention on top of the name.
        let q = self.project(&n("linear_q.weight"), normed, dk, false)?;
        let k = self.project(&n("linear_k.weight"), normed, dk, false)?;
        let v = self.project(&n("linear_v.weight"), normed, dv, false)?;
        // The gate reaches `silu` and therefore has to arrive in Q[`K`], not on the code grid —
        // `MatMulRescale` rather than `MatMulRequant`, for the same reason the FFN's gate uses it.
        // With `Requant` the row clamps at the code rail and `silu` sees a value four orders down,
        // which is what the probe showed: a gate of 2 where the reference says 6.9.
        let z = self.project(&n("linear_z.weight"), normed, dv, true)?;

        // The four-tap causal convolution over the concatenated channels. The window is the
        // cache's, oldest first; a fresh sequence sees zeros before it, which is what "causal"
        // means at the start.
        let width = 2 * dk + dv;
        let mut current = Vec::with_capacity(width);
        current.extend_from_slice(&q);
        current.extend_from_slice(&k);
        current.extend_from_slice(&v);
        let window = &mut cache.conv[li];
        window.remove(0);
        window.push(current);
        let flat: Vec<i32> = window.iter().flatten().copied().collect();
        let taps: Vec<i32> = a.tensor_sized(&n("linear_conv.weight"), s.conv_kernel * width)?.iter().map(|c| *c as i32).collect();
        // `q36_ssm_conv` lands in Q[`K`] — `silu`'s domain — and the requantization after it is
        // what puts the activation back on the code grid the delta rule reads. An earlier version
        // narrowed to codes first and handed `silu` a code row as if it were Q[`K`], which is a
        // different function: at a code scale well below Q[`K`] every input to the nonlinearity is
        // a tiny fraction of what it should be and `silu` degenerates to the linear `x/2`.
        let convolved =
            q36_ssm_conv(&flat, &taps, width, &a.params_sized(&n("linear_conv.a16"), width)?).map_err(refuse("ssm_conv"))?;
        let activated =
            a16_requant(&silu(&convolved), &a.params_sized(&n("linear_conv_act.a16"), width)?).map_err(refuse("conv_silu"))?;

        probe.push((n("linear_qkv"), q.iter().chain(&k).chain(&v).copied().collect()));
        probe.push((n("linear_conv"), activated.clone()));
        let (qc, rest) = activated.split_at(dk);
        let (kc, vc) = rest.split_at(dk);

        // The gates. `decay = exp(−c · softplus(dt + dt_bias))` with `c` the head's registered
        // coefficient; `beta = sigmoid(b)`. Both projections are one lane per value head, and the
        // bias is a tensor of its own — see [`cal::dt_bias`] for why it cannot be dropped.
        let dt = self.project(&n("linear_dt.weight"), normed, s.linear_v_heads, true)?;
        let beta_raw = self.project(&n("linear_beta.weight"), normed, s.linear_v_heads, true)?;
        let decay_c = a.param_rows(&n("linear_decay_c.a16"))?;
        let dt_bias = a.param_rows(&n("linear_dt_bias.a16"))?;
        // **The state's output narrowing is PER HEAD, and that is not a refinement.**
        //
        // The row that leaves the recurrence is normalized per head immediately afterwards, and an
        // RMS norm divides by the head's own magnitude — so a head whose values are small gets few
        // code bits at a shared exponent and the norm then AMPLIFIES its relative error to O(1).
        // Measured, that is the whole of what was left: the state output matches the reference at
        // 0.9999 over the row and its normalization at 0.98, and the gate multiply turns 0.98 into
        // 0.71 because the error lands where the gate is large.
        //
        // Per head is safe where per lane is not: the norm reduces WITHIN a head and the only
        // reduction across heads is the output projection, which happens after the multiply, by
        // which point the row is back on one exponent.
        let read_rows = a.param_rows(&n("linear_read.a16"))?;
        let write_rows = a.param_rows(&n("linear_write.a16"))?;
        let out_rows = a.param_rows(&n("linear_out.a16"))?;
        let delta_rows = a.param_rows(&n("linear_delta.a16"))?;
        let per_head = |rows: &[A16QuantParams], vh: usize| -> A16QuantParams {
            if rows.len() == 1 { rows[0] } else { rows[vh.min(rows.len() - 1)] }
        };

        let mut out = Vec::with_capacity(dv);
        let mut decays: Vec<i32> = Vec::with_capacity(s.linear_v_heads);
        let mut betas: Vec<i32> = Vec::with_capacity(s.linear_v_heads);
        for vh in 0..s.linear_v_heads {
            // `vh % n_k`, not `vh / (n_v/n_k)`: the heads tile, they do not group. See the
            // reference for what the other reading costs.
            let kh = vh % s.linear_k_heads;
            let unit_k = q36_l2_norm(&kc[kh * hd..(kh + 1) * hd]).map_err(refuse("l2_k"))?;
            let unit_q = q36_l2_norm(&qc[kh * hd..(kh + 1) * hd]).map_err(refuse("l2_q"))?;
            let vslice = &vc[vh * hd..(vh + 1) * hd];
            let c = decay_c.get(vh.min(decay_c.len().saturating_sub(1))).map(|p| p.zero).unwrap_or(0);
            let biased =
                dt[vh].saturating_add(dt_bias.get(vh.min(dt_bias.len().saturating_sub(1))).map(|p| p.zero).unwrap_or(0) as i32);
            let decay = q36_decay(biased, c);
            let beta = q36_sigmoid_gate(&[beta_raw[vh]])[0] as i64;
            decays.push(decay as i32);
            betas.push(beta as i32);
            let gdn_params = Qwen36GdnParamsV1 {
                read: per_head(&read_rows, vh),
                delta: per_head(&delta_rows, vh),
                // The write is a shift, carried in the triple's `zero` so the store keeps one
                // wire format.
                write_shift: per_head(&write_rows, vh).zero as i32,
                out: per_head(&out_rows, vh),
            };
            let head_out =
                q36_gdn_step(&mut cache.gdn[li][vh], &unit_k, vslice, &unit_q, decay, beta, gdn_params).map_err(refuse("gdn_step"))?;
            out.extend(head_out);
        }

        probe.push((n("linear_decay"), decays));
        probe.push((n("linear_beta_gate"), betas));
        probe.push((n("linear_state_out"), out.clone()));
        // The LAST head's state, which is the one the reference's per-head site keeps.
        probe.push((n("linear_state"), cache.gdn[li].last().map(|st| st.s.clone()).unwrap_or_default()));
        // The output gate: RMS-normalized PER HEAD, then multiplied by `silu(z)` — a gate on the
        // value stream rather than on the logits, so it is `MulElem` and not a softmax.
        //
        // **Per head.** `ssm_norm.weight` is `[head_dim]`, which is the model saying so, and a norm
        // over the whole 4,096-wide row divides all thirty-two heads by one shared RMS. The
        // magnitudes barely move — that is what a norm does — so this does not show up as a scale
        // error; it shows up as the arm computing a different function, which is exactly what a
        // rank correlation of 0.15 against the reference looked like.
        // **Both factors stay wide until after the multiply.**
        //
        // The gate multiply is a cancellation: measured, its output's rms is thirteen times below
        // the product of its factors' rms, because the two rows are anticorrelated in magnitude.
        // Quantization error is uniform in ABSOLUTE terms, so narrowing each factor to sixteen
        // bits first puts a large RELATIVE error on exactly the lanes where the other factor is
        // large. It took this site from a cosine of 0.98 against the float reference to 0.71 —
        // and it does not read as a scale error, because the magnitudes come out right.
        let norm_params = a.params_sized(&n("linear_norm.a16"), dv)?;
        // Per head, because the head's own output exponent is what `eps` has to be stated against.
        let norm_eps = a.param_rows(&n("linear_norm_eps.a16"))?;
        let mut normed_out = Vec::with_capacity(dv);
        for vh in 0..s.linear_v_heads {
            let head = &out[vh * hd..(vh + 1) * hd];
            let unit = q36_rms_norm_wide(head, norm_eps[vh.min(norm_eps.len() - 1)]).map_err(refuse("gdn_norm"))?;
            normed_out.extend(q36_rescale_row(&unit, &norm_params[vh * hd..(vh + 1) * hd]).map_err(refuse("gdn_norm_req"))?);
        }
        let gate = silu(&z);
        probe.push((n("linear_z"), z.clone()));
        probe.push((n("linear_normed"), normed_out.clone()));
        probe.push((n("linear_gate_act"), gate.clone()));
        let gated = q36_mul_wide(&normed_out, &gate, &a.params_sized(&n("linear_gated.a16"), dv)?).map_err(refuse("gdn_gated"))?;
        probe.push((n("linear_gated"), gated.clone()));

        self.project(&n("linear_o.weight"), &gated, s.d_model, false)
    }

    /// **The gated-attention arm.** QK-norm per head before the rotation, partial rotation, GQA
    /// softmax attention, then an elementwise `sigmoid` gate on the output.
    fn full_arm(
        &self,
        cache: &mut Qwen36Cache,
        li: usize,
        normed: &[i32],
        position: usize,
        probe: &mut Vec<(String, Vec<i32>)>,
    ) -> Result<Vec<i32>, Qwen36Error> {
        let a = self.artifact;
        let s = &a.shape;
        let (d, hd) = (s.d_model, s.head_dim);
        let q_dim = s.n_heads * hd;
        let kv_dim = s.kv_dim();
        let n = |suffix: &str| format!("blk.{li}.{suffix}");
        let (cos_row, sin_row) = a.rope.row(position).ok_or(Qwen36Error::Position)?;
        let pairs = s.rotary_dim / 2;
        if cos_row.len() < pairs {
            return Err(Qwen36Error::Position);
        }
        let (cos_row, sin_row) = (&cos_row[..pairs], &sin_row[..pairs]);

        // Hybrid members (`attn_output_gate()`): the q projection is double width and the second
        // half is the gate, stored as two tensors so a court opening addresses either half by
        // name. The qwen3moe members are plain q — no gate tensor exists in their artifact.
        let q = self.project(&n("attn_q.weight"), normed, q_dim, false)?;
        let gate_raw = if s.attn_output_gate() { Some(self.project(&n("attn_gate.weight"), normed, q_dim, true)?) } else { None };
        let k = self.project(&n("attn_k.weight"), normed, kv_dim, false)?;
        let v = self.project(&n("attn_v.weight"), normed, kv_dim, false)?;
        probe.push((n("attn_q"), q.clone()));
        if let Some(gate_raw) = &gate_raw {
            probe.push((n("attn_gate"), gate_raw.clone()));
        }
        probe.push((n("attn_v"), v.clone()));

        // QK-norm: RMSNorm PER HEAD, before the rotation. Normalizing the whole row instead would
        // couple the heads, and doing it after the rotation would normalize away part of what the
        // rotation encodes.
        let per_head_norm = |row: &[i32], heads: usize, name: &str| -> Result<Vec<i32>, Qwen36Error> {
            let params = a.params_sized(name, hd)?;
            let mut out = Vec::with_capacity(row.len());
            for head in 0..heads {
                let slice = &row[head * hd..(head + 1) * hd];
                let unit = a16_rms_norm(slice, s.eps_q).map_err(refuse("qk_norm"))?;
                out.extend(a16_requant(&unit, &params).map_err(refuse("qk_norm_req"))?);
            }
            Ok(out)
        };
        let q = per_head_norm(&q, s.n_heads, &n("attn_q_norm.a16"))?;
        let k = per_head_norm(&k, s.n_kv_heads, &n("attn_k_norm.a16"))?;

        let clamp = a.one_param(&n("attn_rope.a16"))?;
        let q = q36_rope_partial(&q, hd, s.rotary_dim, cos_row, sin_row, clamp).map_err(refuse("rope_q"))?;
        let k = q36_rope_partial(&k, hd, s.rotary_dim, cos_row, sin_row, clamp).map_err(refuse("rope_k"))?;
        probe.push((n("attn_q_rot"), q.clone()));
        probe.push((n("attn_k_rot"), k.clone()));

        cache.keys[li].push(k);
        cache.values[li].push(v);
        let history = cache.keys[li].len();
        let mut k_series = Vec::with_capacity(history * kv_dim);
        let mut v_series = Vec::with_capacity(history * kv_dim);
        for j in 0..history {
            k_series.extend_from_slice(&cache.keys[li][j]);
            v_series.extend_from_slice(&cache.values[li][j]);
        }

        let logit_p = a.one_param(&n("attn_logits.a16"))?;
        let up_bits = a.scalar(&n("attn_softmax_up.a16"))?.clamp(0, 62) as u8;
        let probs_p = a.one_param(&n("attn_probs.a16"))?;
        let value_p = a.one_param(&n("attn_values.a16"))?;
        let scores = a16_attn_scores(&q, &k_series, s.n_heads, s.n_kv_heads, hd, &vec![logit_p; s.n_heads * history])
            .map_err(refuse("attn_scores"))?;
        let probs = a16_softmax_rows(&scores, history, up_bits).map_err(refuse("attn_softmax"))?;
        let narrowed = a16_requant(&probs, &vec![probs_p; s.n_heads * history]).map_err(refuse("attn_probs"))?;
        let attn = a16_attn_values(&narrowed, &v_series, s.n_heads, s.n_kv_heads, hd, &vec![value_p; q_dim])
            .map_err(refuse("attn_values"))?;

        // The output gate. `sigmoid` of the gate row, applied elementwise before the projection —
        // hybrid members only; on a gateless member the attention row goes to the projection as
        // it is, exactly as the stripped profile says.
        let gated = match &gate_raw {
            Some(gate_raw) => {
                let gate = q36_sigmoid_gate(gate_raw);
                q36_gate_apply(&attn, &gate, a.one_param(&n("attn_gated.a16"))?).map_err(refuse("attn_gate_apply"))?
            }
            None => attn.clone(),
        };
        probe.push((n("attn_values"), attn));
        if gate_raw.is_some() {
            probe.push((n("attn_gated"), gated.clone()));
        }

        // Through `project`, not the plain matmul: every weight tensor this converter writes
        // carries a per-32 group exponent, and a matmul that ignores it reads codes anchored
        // `2^QWEN36_MAX_GROUP_EXP` below their true scale. The output projection was the one site
        // still calling the dense form, and it did not read as an approximation — the dot product
        // came out a million times small and the narrowing rounded the whole row to **zero**.
        let out = self.project(&n("attn_o.weight"), &gated, d, false)?;
        let _ = q_dim;
        probe.push((n("attn_out"), out.clone()));
        Ok(out)
    }

    /// **The mixture.** Route, run the eight chosen experts and the always-on shared one, combine.
    ///
    /// The experts are run one at a time rather than gathered into a dense matmul: at 8 of 256 the
    /// gather is 97 % waste, and the whole point of the architecture is that only the chosen
    /// weights are read. That is also what makes the MoE the part a memory map serves best.
    fn moe(&self, li: usize, normed: &[i32], probe: &mut Vec<(String, Vec<i32>)>) -> Result<Vec<i32>, Qwen36Error> {
        let a = self.artifact;
        let s = &a.shape;
        let d = s.d_model;
        let n = |suffix: &str| format!("blk.{li}.{suffix}");

        let router = self.project(&n("ffn_router.weight"), normed, s.n_experts, true)?;
        // The router's logits are narrowed to codes before the selection, because the tie rule is
        // defined on what the class commits to and a wider intermediate would let two
        // implementations disagree about a tie that the committed row does not have.
        let router_codes = a16_requant(&router, &a.params_sized(&n("ffn_router.a16"), s.n_experts)?).map_err(refuse("router_req"))?;
        // **The widening is class data per layer, not a shape constant.** `softmax_shifted` needs
        // to know how far below Q[`K`] the committed router codes sit, and that is a property of
        // the layer's measured logit range. Reading it from the shape used a single number for
        // forty layers, which is a temperature error of up to a factor of sixty-four — enough to
        // make the router select nearly uniformly or nearly one-hot.
        let up = a.scalar(&n("ffn_router_up.a16"))?.clamp(0, 62) as u8;
        let routed = q36_router_topk(&router_codes, s.experts_per_token, up).map_err(refuse("router_topk"))?;

        // **The prefetch.** The routing is committed and no expert has run yet, so this is the
        // one moment where every byte the mixture will read is known and none of it is needed
        // instantly — which is exactly when to ask for it.
        a.admit_experts(li, &routed.iter().map(|r| r.expert as usize).collect::<Vec<_>>());
        let mut outputs = Vec::with_capacity(routed.len() * d);
        let mut weights = Vec::with_capacity(routed.len());
        for r in &routed {
            let e = r.expert as usize;
            outputs.extend(self.expert(li, e, normed, s.moe_dim, "expert")?);
            weights.push(r.weight_q);
        }
        probe.push((n("ffn_router"), router_codes.clone()));
        // The selection itself, so a routed output that disagrees with the reference can be told
        // apart from one that chose a different eighth expert. Ids and weights are separate rows
        // because a cosine over ids means nothing and a cosine over weights means everything.
        probe.push((n("ffn_choice"), routed.iter().map(|r| r.expert as i32).collect()));
        probe.push((n("ffn_weight"), weights.clone()));
        probe.push((n("ffn_expert_out"), outputs.clone()));
        let combined = q36_moe_combine(&outputs, &weights, d, a.one_param(&n("ffn_combine.a16"))?).map_err(refuse("moe_combine"))?;
        probe.push((n("ffn_routed"), combined.clone()));

        // The shared expert, always on, behind its own scalar gate — hybrid members only. The
        // qwen3moe members route through the mixture alone, and their stripped profile has no
        // fold to feed: the combine goes straight to the narrowing.
        let sum = if s.has_shared_expert() {
            let shared = self.expert(li, usize::MAX, normed, s.shared_dim, "shared")?;
            let shared_gate_raw = self.project(&n("ffn_shared_gate.weight"), normed, 1, true)?;
            let g = q36_sigmoid_gate(&shared_gate_raw)[0];
            // The shared expert's output is wide like the routed ones, so the scalar gate goes
            // through the wide product and lands on the same code grid the combine did.
            let shared_gated = q36_mul_wide(&shared, &vec![g; d], &vec![a.one_param(&n("ffn_shared_gated.a16"))?; d])
                .map_err(refuse("shared_apply"))?;
            probe.push((n("ffn_shared_out"), shared.clone()));
            a16_add_elem(&combined, &shared_gated).map_err(refuse("moe_add"))?
        } else {
            combined.clone()
        };
        let out = a16_requant(&sum, &a.params_sized(&n("ffn_moe_out.a16"), d)?).map_err(refuse("moe_out"))?;
        probe.push((n("ffn_moe_out"), out.clone()));
        Ok(out)
    }

    /// One SwiGLU expert. `which` is the expert index, or `usize::MAX` for the shared one, which
    /// names its tensors differently and has its own intermediate width.
    fn expert(&self, li: usize, which: usize, x: &[i32], mid: usize, kind: &str) -> Result<Vec<i32>, Qwen36Error> {
        let a = self.artifact;
        let d = a.shape.d_model;
        // **`ffn_shared_expert`, not `ffn_shared`.** The shared expert's own gate projection would
        // then be `blk.N.ffn_shared_gate.weight` — the exact name the mixture's SCALAR gate already
        // uses. A `BTreeMap` keyed by strings has no way to notice, and the first version of this
        // silently handed a 512-value expert tensor to a 32-value scalar read. The engine caught it
        // only because it checks sizes; a collision between two rows of the same width would have
        // run to completion and computed something else.
        let base = if which == usize::MAX { format!("blk.{li}.ffn_shared_expert") } else { format!("blk.{li}.ffn_expert.{which}") };
        let _ = kind;

        let gate = self.project(&format!("{base}_gate.weight"), x, mid, true)?;
        let up = self.project(&format!("{base}_up.weight"), x, mid, false)?;
        // Same cancellation, same fix: `silu(gate)` stays in Q[`K`] and the product narrows once.
        let activated =
            q36_rescale_row(&silu(&gate), &a.params_sized(&format!("{base}_silu.a16"), mid)?).map_err(refuse("expert_silu"))?;
        let gated =
            q36_mul_wide(&activated, &up, &a.params_sized(&format!("{base}_gated.a16"), mid)?).map_err(refuse("expert_gated"))?;
        // Wide out: the combine sums eight of these and the sum cancels, so each row keeps its
        // precision until the one narrowing after the sum.
        self.project(&format!("{base}_down.weight"), &gated, d, true)
    }
}

// -------------------------------------------------------------------------------------------
// The artifact file — a directory the runtime maps rather than reads
// -------------------------------------------------------------------------------------------

/// Magic and version. The format is deliberately dull: fixed-width little-endian scalars,
/// length-prefixed names, and one page-aligned blob at the end.
pub const QWEN36_FILE_MAGIC: &[u8; 8] = b"PALWQ361";

/// Where the weight region starts. Page-aligned so the mapping's offsets are the file's.
const DATA_ALIGNMENT: usize = 16384;

/// **The writer, which is streaming on purpose.**
///
/// A 33 GiB artifact cannot be built in memory and then written; it has to be written as it is
/// produced. The directory is therefore computed BEFORE any data — every tensor's length is known
/// from the shape, so the offsets are arithmetic — and the tensors then arrive in the order the
/// plan declared them.
pub struct Qwen36Writer {
    file: std::io::BufWriter<std::fs::File>,
    plan: Vec<(String, usize)>,
    next: usize,
    written: usize,
    /// Where each parameter row's VALUE bytes start in the file, so a calibrating converter can
    /// rewrite them after the pass that measured them.
    ///
    /// A quantization scale is a statement about a range and a range has to be observed, but the
    /// weights can be quantized without one — the codes are a per-output-channel property of the
    /// weight and nothing else. So one pass writes the codes and measures the ranges, and the
    /// triples are patched in place at the end. The layout does not change: only the seventeen
    /// bytes of each triple do, which is what makes this a patch rather than a second format.
    param_offsets: std::collections::BTreeMap<String, (usize, usize)>,
    path: std::path::PathBuf,
}

impl Qwen36Writer {
    /// `plan` is every tensor in the order it will be supplied, with its length in codes.
    pub fn create(
        path: &std::path::Path,
        shape: &Qwen36ShapeV1,
        rope: &RopeTableV1,
        params: &BTreeMap<String, Vec<u8>>,
        plan: Vec<(String, usize)>,
    ) -> std::io::Result<Self> {
        use std::io::Write;
        let mut out = Vec::with_capacity(1 << 20);
        out.extend_from_slice(QWEN36_FILE_MAGIC);
        write_shape(&mut out, shape);
        write_usize(&mut out, rope.d_head);
        write_usize(&mut out, rope.max_position);
        write_i32s(&mut out, &rope.cos_q);
        write_i32s(&mut out, &rope.sin_q);
        write_usize(&mut out, params.len());
        let mut param_offsets = std::collections::BTreeMap::new();
        for (name, bytes) in params {
            write_name(&mut out, name);
            write_usize(&mut out, bytes.len());
            param_offsets.insert(name.clone(), (out.len(), bytes.len()));
            out.extend_from_slice(bytes);
        }
        write_usize(&mut out, plan.len());
        // The offsets are relative to the data region, which starts at the first aligned boundary
        // after the header. The header's own length depends on the directory, and the directory's
        // entries are fixed-width once the names are written — so the region start is computed
        // after the whole header is laid out, and the offsets do not depend on it.
        let mut offset = 0usize;
        for (name, len) in &plan {
            write_name(&mut out, name);
            write_usize(&mut out, offset);
            write_usize(&mut out, *len);
            offset += *len;
        }
        let data_start = out.len().next_multiple_of(DATA_ALIGNMENT);
        // The region start is recorded so the reader does not have to reproduce the padding rule.
        let start_field = out.len();
        write_usize(&mut out, 0);
        let data_start = (start_field + 8).next_multiple_of(DATA_ALIGNMENT).max(data_start);
        out[start_field..start_field + 8].copy_from_slice(&(data_start as u64).to_le_bytes());
        out.resize(data_start, 0);

        let mut file = std::io::BufWriter::with_capacity(1 << 22, std::fs::File::create(path)?);
        file.write_all(&out)?;
        Ok(Self { file, plan, next: 0, written: 0, param_offsets, path: path.to_path_buf() })
    }

    /// Append the next tensor. Refuses a name or a length the plan did not declare — the
    /// directory is already on disk, so a tensor that arrives out of order would be silently
    /// mis-addressed by every reader.
    pub fn push(&mut self, name: &str, codes: &[i8]) -> std::io::Result<()> {
        use std::io::Write;
        let Some((want_name, want_len)) = self.plan.get(self.next) else {
            return Err(std::io::Error::other(format!("the plan has no slot for {name}")));
        };
        if want_name != name || *want_len != codes.len() {
            return Err(std::io::Error::other(format!(
                "the plan expects {want_name} with {want_len} codes and got {name} with {}",
                codes.len()
            )));
        }
        // SAFETY-free reinterpretation: `i8` and `u8` have the same layout, and this is a write.
        let bytes: &[u8] = unsafe { std::slice::from_raw_parts(codes.as_ptr() as *const u8, codes.len()) };
        self.file.write_all(bytes)?;
        self.next += 1;
        self.written += codes.len();
        Ok(())
    }

    /// Finish. Refuses to close a file the plan did not fill.
    pub fn finish(mut self) -> std::io::Result<usize> {
        use std::io::Write;
        if self.next != self.plan.len() {
            return Err(std::io::Error::other(format!("the plan declared {} tensors and {} arrived", self.plan.len(), self.next)));
        }
        self.file.flush()?;
        Ok(self.written)
    }

    /// Finish, rewriting the parameter values measured during the pass.
    ///
    /// Every name must be one the header already declared and every replacement must be the same
    /// length: the directory is on disk and a row that changed width would move every row after
    /// it. A name that was not declared is an error rather than an append, because a parameter the
    /// header does not list is a parameter no reader will ever look for.
    pub fn finish_with_params(mut self, measured: &BTreeMap<String, Vec<u8>>) -> std::io::Result<usize> {
        use std::io::{Seek, SeekFrom, Write};
        if self.next != self.plan.len() {
            return Err(std::io::Error::other(format!("the plan declared {} tensors and {} arrived", self.plan.len(), self.next)));
        }
        self.file.flush()?;
        drop(self.file);
        let mut file = std::fs::OpenOptions::new().write(true).open(&self.path)?;
        for (name, bytes) in measured {
            let Some((offset, len)) = self.param_offsets.get(name) else {
                return Err(std::io::Error::other(format!("the header does not declare a parameter row {name}")));
            };
            if *len != bytes.len() {
                return Err(std::io::Error::other(format!(
                    "parameter {name} was declared {len} bytes and the measurement is {}",
                    bytes.len()
                )));
            }
            file.seek(SeekFrom::Start(*offset as u64))?;
            file.write_all(bytes)?;
        }
        file.flush()?;
        Ok(self.written)
    }
}

fn write_usize(out: &mut Vec<u8>, v: usize) {
    out.extend_from_slice(&(v as u64).to_le_bytes());
}

fn write_name(out: &mut Vec<u8>, name: &str) {
    write_usize(out, name.len());
    out.extend_from_slice(name.as_bytes());
}

fn write_i32s(out: &mut Vec<u8>, v: &[i32]) {
    write_usize(out, v.len());
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
}

fn write_shape(out: &mut Vec<u8>, s: &Qwen36ShapeV1) {
    write_usize(out, s.layer_types.len());
    for k in &s.layer_types {
        out.push(match k {
            Qwen36LayerKind::LinearAttention => 0,
            Qwen36LayerKind::FullAttention => 1,
        });
    }
    for v in [
        s.d_model,
        s.n_heads,
        s.n_kv_heads,
        s.head_dim,
        s.rotary_dim,
        s.linear_k_heads,
        s.linear_v_heads,
        s.linear_head_dim,
        s.conv_kernel,
        s.n_experts,
        s.experts_per_token,
        s.moe_dim,
        s.shared_dim,
        s.vocab,
        s.max_position,
    ] {
        write_usize(out, v);
    }
    out.extend_from_slice(&s.eps_q.to_le_bytes());
    out.push(s.router_up_bits);
}

struct HeaderReader<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> HeaderReader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.i.checked_add(n)?;
        if end > self.b.len() {
            return None;
        }
        let out = &self.b[self.i..end];
        self.i = end;
        Some(out)
    }
    fn usize(&mut self) -> Option<usize> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?) as usize)
    }
    fn i64(&mut self) -> Option<i64> {
        Some(i64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn name(&mut self) -> Option<String> {
        let n = self.usize()?;
        Some(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
    fn i32s(&mut self) -> Option<Vec<i32>> {
        let n = self.usize()?;
        Some(self.take(n * 4)?.chunks_exact(4).map(|c| i32::from_le_bytes(c.try_into().expect("4"))).collect())
    }
}

/// What the header says: the shape, the rotary table, where every parameter row's value bytes
/// and every tensor's codes sit in the file. Parsed from a byte prefix of the file.
struct ParsedHeaderV1 {
    shape: Qwen36ShapeV1,
    rope: RopeTableV1,
    /// `(name, offset, len)` — absolute offsets of each row's value bytes.
    params: Vec<(String, usize, usize)>,
    /// `(name, offset, len)` — absolute offsets of each tensor's codes.
    tensors: Vec<(String, usize, usize)>,
}

enum HeaderFailure {
    /// The bytes ended before the field named did — a short prefix, or a truncated file; the
    /// caller, which knows how much of the file it handed in, tells the two apart.
    Short(&'static str),
    Bad(String),
}

fn parse_header_v1(bytes: &[u8]) -> Result<ParsedHeaderV1, HeaderFailure> {
    let bad = |what: &str| HeaderFailure::Bad(format!("artifact file: {what}"));
    let mut r = HeaderReader { b: bytes, i: 0 };
    macro_rules! need {
        ($e:expr, $what:expr) => {
            match $e {
                Some(v) => v,
                None => return Err(HeaderFailure::Short($what)),
            }
        };
    }
    if need!(r.take(8), "magic") != QWEN36_FILE_MAGIC.as_slice() {
        return Err(bad("not a PALW-QWEN36 artifact"));
    }
    let n_layers = need!(r.usize(), "layer count");
    let mut layer_types = Vec::with_capacity(n_layers);
    for _ in 0..n_layers {
        layer_types.push(match need!(r.take(1), "layer kind")[0] {
            0 => Qwen36LayerKind::LinearAttention,
            1 => Qwen36LayerKind::FullAttention,
            _ => return Err(bad("a layer kind this build does not read")),
        });
    }
    let mut field = || r.usize();
    let shape = Qwen36ShapeV1 {
        layer_types,
        d_model: need!(field(), "shape"),
        n_heads: need!(field(), "shape"),
        n_kv_heads: need!(field(), "shape"),
        head_dim: need!(field(), "shape"),
        rotary_dim: need!(field(), "shape"),
        linear_k_heads: need!(field(), "shape"),
        linear_v_heads: need!(field(), "shape"),
        linear_head_dim: need!(field(), "shape"),
        conv_kernel: need!(field(), "shape"),
        n_experts: need!(field(), "shape"),
        experts_per_token: need!(field(), "shape"),
        moe_dim: need!(field(), "shape"),
        shared_dim: need!(field(), "shape"),
        vocab: need!(field(), "shape"),
        max_position: need!(field(), "shape"),
        eps_q: need!(r.i64(), "eps"),
        router_up_bits: need!(r.take(1), "router bits")[0],
    };
    shape.validate().map_err(|e| bad(&format!("{e:?}")))?;

    let rope = RopeTableV1 {
        d_head: need!(r.usize(), "rope d_head"),
        max_position: need!(r.usize(), "rope max_position"),
        cos_q: need!(r.i32s(), "rope cos"),
        sin_q: need!(r.i32s(), "rope sin"),
    };

    let n_params = need!(r.usize(), "param count");
    let mut params = Vec::with_capacity(n_params);
    for _ in 0..n_params {
        let name = need!(r.name(), "param name");
        let n = need!(r.usize(), "param length");
        let at = r.i;
        need!(r.take(n), "param bytes");
        params.push((name, at, n));
    }

    let n_tensors = need!(r.usize(), "tensor count");
    let mut entries = Vec::with_capacity(n_tensors);
    for _ in 0..n_tensors {
        let name = need!(r.name(), "tensor name");
        let offset = need!(r.usize(), "tensor offset");
        let len = need!(r.usize(), "tensor length");
        entries.push((name, offset, len));
    }
    let data_start = need!(r.usize(), "data start");
    let mut tensors = Vec::with_capacity(entries.len());
    for (name, offset, len) in entries {
        let absolute = data_start.checked_add(offset).ok_or_else(|| bad("a tensor offset overflows"))?;
        tensors.push((name, absolute, len));
    }
    Ok(ParsedHeaderV1 { shape, rope, params, tensors })
}

/// Open an artifact file with the page cache deciding residency — the path before ADR-0112,
/// kept as it was so the two can be measured against each other. The header is parsed and the
/// weight region is mapped, so opening a 33 GiB artifact costs the header and no more.
pub fn open_artifact(path: &std::path::Path) -> Result<Qwen36ArtifactV1, Qwen36Error> {
    open_artifact_with_residency(path, Qwen36ResidencyPolicyV1::PageCache)
}

/// **Open an artifact file under a residency policy** (ADR-0112).
///
/// With a budget: the header is read through the file descriptor (the 33 GiB class's is
/// 0.71 GiB of parameter rows, which faulted through the mapping is a minute of 4 KiB faults),
/// the budget is checked against the class's floor before anything else happens, the always-set
/// is read into memory this process owns — in parallel, seconds for 1.86 GiB — and every routed
/// expert's extents are kept for the misses to read from. The file stays mapped: it is where the
/// embedding table and, with no residency, everything else is read.
pub fn open_artifact_with_residency(path: &std::path::Path, policy: Qwen36ResidencyPolicyV1) -> Result<Qwen36ArtifactV1, Qwen36Error> {
    use rayon::prelude::*;
    let map = crate::mmap::ReadOnlyMap::open(path).map_err(|e| Qwen36Error::BadParams(format!("{}: {e}", path.display())))?;
    map.advise_random();
    let unreadable = |what: &str, e: std::io::Error| Qwen36Error::Unreadable(format!("{}: {what}: {e}", path.display()));
    let ended = |what: &str| Qwen36Error::BadParams(format!("artifact file: {what}: the file ends inside its header"));
    let header = match policy {
        Qwen36ResidencyPolicyV1::PageCache => parse_header_v1(map.as_bytes()).map_err(|f| match f {
            HeaderFailure::Short(what) => ended(what),
            HeaderFailure::Bad(m) => Qwen36Error::BadParams(m),
        })?,
        _ => {
            // A prefix through the file descriptor, doubled until the header fits in it.
            let mut n = (64usize << 20).min(map.len());
            loop {
                let prefix = map.read_u8_at(0, n).map_err(|e| unreadable("the header", e))?;
                match parse_header_v1(&prefix) {
                    Ok(header) => break header,
                    Err(HeaderFailure::Short(_)) if n < map.len() => n = n.saturating_mul(2).min(map.len()),
                    Err(HeaderFailure::Short(what)) => return Err(ended(what)),
                    Err(HeaderFailure::Bad(m)) => return Err(Qwen36Error::BadParams(m)),
                }
            }
        }
    };
    let ParsedHeaderV1 { shape, rope, params, tensors } = header;
    let mut directory = BTreeMap::new();
    for (name, offset, len) in &tensors {
        directory.insert(name.clone(), (*offset, *len));
    }
    let weight_bytes: usize = tensors.iter().map(|(_, _, len)| *len).sum();

    let Some(budget) = policy.budget_for(weight_bytes as u64) else {
        // Every parameter row owned, read as the old path read it.
        let bytes = map.as_bytes();
        let mut owned = BTreeMap::new();
        for (name, offset, len) in params {
            let row = bytes.get(offset..offset + len).ok_or_else(|| ended("param bytes"))?;
            owned.insert(name, row.to_vec());
        }
        return Ok(Qwen36ArtifactV1 { shape, store: Store::Mapped { map, directory }, params: owned, rope, residency: None });
    };
    let budget = usize::try_from(budget).map_err(|_| Qwen36Error::Residency("the residency budget does not fit a usize".into()))?;

    // The two tiers, told apart by name (ADR-0112 Decision 3).
    let mut experts: std::collections::HashMap<(usize, usize), ExpertExtentsV1> = std::collections::HashMap::new();
    let mut expert_param_extents = BTreeMap::new();
    let mut owned_params = BTreeMap::new();
    for (name, offset, len) in params {
        match expert_key_v1(&name) {
            Some((key, _)) => {
                let entry = experts.entry(key).or_default();
                entry.params.push((name.clone(), offset, len));
                entry.bytes += len;
                expert_param_extents.insert(name, (offset, len));
            }
            None => {
                owned_params.insert(name.clone(), map.read_u8_at(offset, len).map_err(|e| unreadable(&name, e))?);
            }
        }
    }
    let mut pinned_extents = Vec::new();
    for (name, offset, len) in &tensors {
        match expert_key_v1(name) {
            Some((key, _)) => {
                let entry = experts.entry(key).or_default();
                entry.tensors.push((name.clone(), *offset, *len));
                entry.bytes += len;
            }
            None if name == "token_embd.weight" => {}
            None => pinned_extents.push((name.clone(), *offset, *len)),
        }
    }
    let expert_bytes = experts.values().map(|e| e.bytes).max().unwrap_or(0);
    let expert_layers = experts.keys().map(|(layer, _)| *layer).collect::<std::collections::BTreeSet<_>>().len();
    let pinned_bytes: usize = pinned_extents.iter().map(|(_, _, len)| *len).sum();
    let one_token = expert_bytes.saturating_mul(shape.experts_per_token).saturating_mul(expert_layers);
    let floor = pinned_bytes.saturating_add(one_token);
    if budget < floor {
        return Err(Qwen36Error::Residency(format!(
            "a resident budget of {budget} bytes is below this class's floor of {floor}: the always-set is {pinned_bytes} bytes and \
             one token's routed experts are {one_token} ({} experts of {expert_bytes} bytes in each of {expert_layers} layers); the \
             smallest budget this class runs in is {floor} (ADR-0112 Decision 3)",
            shape.experts_per_token
        )));
    }

    // The always-set, read once — in parallel, because the reads are the whole cost of opening
    // under a budget and the device has a queue.
    let pinned: std::collections::HashMap<String, std::sync::Arc<Vec<i8>>> = pinned_extents
        .par_iter()
        .map(|(name, offset, len)| {
            map.read_i8_at(*offset, *len).map(|codes| (name.clone(), std::sync::Arc::new(codes))).map_err(|e| unreadable(name, e))
        })
        .collect::<Result<_, _>>()?;

    let residency = Qwen36ResidencyV1 {
        budget,
        pinned_bytes,
        pinned,
        experts,
        expert_param_extents,
        expert_bytes,
        expert_layers,
        experts_per_token: shape.experts_per_token,
        inner: std::sync::Mutex::new(ResidencyInnerV1 {
            held: std::collections::HashMap::new(),
            order: std::collections::VecDeque::new(),
            bytes: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
            bytes_read: pinned_bytes as u64,
        }),
    };
    Ok(Qwen36ArtifactV1 { shape, store: Store::Mapped { map, directory }, params: owned_params, rope, residency: Some(residency) })
}

/// The parameter store a writer needs, taken out of an in-memory artifact.
impl Qwen36ArtifactV1 {
    pub fn params_map(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.params
    }
}

/// The Qwen3.6-shaped fixture, for tests in other modules of this crate.
#[cfg(test)]
pub(crate) fn test_fixture(layers: usize, experts: usize) -> Qwen36ArtifactV1 {
    tests::fixture(layers, experts)
}

/// A fixture at an ARBITRARY (validated) shape, for tests in other modules of this crate — the
/// interpreter's per-row differential shrinks each catalog member to a runnable geometry, and the
/// members differ structurally (interval, gate, shared expert, expert count), so one hard-coded
/// shape cannot stand for all of them.
#[cfg(test)]
pub(crate) fn test_fixture_for_shape(shape: Qwen36ShapeV1) -> Qwen36ArtifactV1 {
    use crate::artifact::LN_THETA_10000_GEN_Q;
    let rope = RopeTableV1::generate(shape.head_dim, shape.max_position, LN_THETA_10000_GEN_Q).expect("a table");
    let artifact = Qwen36ArtifactV1::new(shape.clone(), rope).expect("a valid shape");
    fill_fixture(artifact, shape)
}

/// **The fixture, for tests in OTHER crates** — the consensus block E2E runs a real
/// Qwen3.6-shaped engine over it, because a block test that faked its execution would prove only
/// that fakes are accepted. Always compiled: `#[cfg(test)]` does not cross crates, and a
/// `testutils` feature would put the block E2E behind a flag nobody runs. It is a FIXTURE and
/// says so in its name; nothing derives a registrable class from it, and a chain that registered
/// its artifact root registered a toy on purpose — a drill, which is the only place this belongs.
pub fn qwen36_dev_fixture(layers: usize, experts: usize) -> Qwen36ArtifactV1 {
    fixture_impl(layers, experts)
}

/// The qwen3moe-flavored fixture: every layer full attention, no gate, no shared expert, full
/// rotation. Same discipline (and the same warning) as [`qwen36_dev_fixture`].
pub fn qwen3moe_dev_fixture(layers: usize, experts: usize) -> Qwen36ArtifactV1 {
    use crate::artifact::LN_THETA_10000_GEN_Q;
    let shape = Qwen36ShapeV1 {
        layer_types: vec![Qwen36LayerKind::FullAttention; layers],
        d_model: 32,
        n_heads: 4,
        n_kv_heads: 2,
        head_dim: 16,
        rotary_dim: 16,
        linear_k_heads: 0,
        linear_v_heads: 0,
        linear_head_dim: 0,
        conv_kernel: 0,
        n_experts: experts,
        experts_per_token: 4,
        moe_dim: 16,
        shared_dim: 0,
        vocab: 64,
        max_position: 32,
        eps_q: 1,
        router_up_bits: 20,
    };
    let rope = RopeTableV1::generate(shape.head_dim, shape.max_position, LN_THETA_10000_GEN_Q).expect("a table");
    let artifact = Qwen36ArtifactV1::new(shape.clone(), rope).expect("a valid qwen3moe shape");
    fill_fixture(artifact, shape)
}

/// The fixture's body, shared by the in-crate door (`tests::fixture`) and the cross-crate one.
fn fixture_impl(layers: usize, experts: usize) -> Qwen36ArtifactV1 {
    use crate::artifact::LN_THETA_10000_GEN_Q;
    let shape = Qwen36ShapeV1 {
        layer_types: (0..layers)
            .map(|i| if (i + 1).is_multiple_of(4) { Qwen36LayerKind::FullAttention } else { Qwen36LayerKind::LinearAttention })
            .collect(),
        d_model: 32,
        n_heads: 4,
        n_kv_heads: 2,
        head_dim: 16,
        rotary_dim: 4,
        linear_k_heads: 2,
        linear_v_heads: 4,
        linear_head_dim: 8,
        conv_kernel: 4,
        n_experts: experts,
        experts_per_token: 4,
        moe_dim: 16,
        shared_dim: 16,
        vocab: 64,
        max_position: 32,
        eps_q: 1,
        router_up_bits: 20,
    };
    let rope = RopeTableV1::generate(shape.head_dim, shape.max_position, LN_THETA_10000_GEN_Q).expect("a table");
    let artifact = Qwen36ArtifactV1::new(shape.clone(), rope).expect("a valid shape");
    fill_fixture(artifact, shape)
}

/// Deterministic weights and workable narrowing params for a fixture of either flavor — the
/// per-layer walk reads the shape, so the gate, the recurrence and the shared expert appear
/// exactly where the shape has them.
fn fill_fixture(mut artifact: Qwen36ArtifactV1, shape: Qwen36ShapeV1) -> Qwen36ArtifactV1 {
    let d = shape.d_model;

    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || -> i8 {
        state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (((state >> 40) & 0xFF) as u8 as i8).saturating_abs().wrapping_sub(64)
    };
    let mut weights = |n: usize| -> Vec<i8> { (0..n).map(|_| next()).collect() };

    // A projection over `fan_in` attenuates by `2^-(8 + bits(fan_in)/2)`; an elementwise site
    // is unity. Same rule as the dense fixture, and for the same reason: one gain everywhere
    // decays the residual stream to zero and the differential would not notice.
    let projection = |fan_in: usize| -> A16QuantParams {
        let bits = usize::BITS - fan_in.max(1).leading_zeros();
        A16QuantParams { multiplier: 1, shift: (8 + bits / 2) as u8, zero: 0 }
    };
    let unity = A16QuantParams { multiplier: 1, shift: 0, zero: 0 };

    artifact = artifact
        .with_tensor("token_embd.weight", weights(shape.vocab * d))
        .with_tensor("output.weight", weights(shape.vocab * d))
        .with_params("embed_lift.a16", &[unity])
        .with_params("final_norm.a16", &[unity])
        .with_params("output.weight.a16", &[projection(d)]);

    for (li, kind) in shape.layer_types.iter().enumerate() {
        let n = |suffix: &str| format!("blk.{li}.{suffix}");
        for row in ["attn_norm.a16", "attn_align.a16", "attn_residual.a16", "ffn_norm.a16", "ffn_align.a16", "ffn_residual.a16"] {
            artifact = artifact.with_params(n(row), &[unity]);
        }
        match kind {
            Qwen36LayerKind::LinearAttention => {
                let (dk, dv, hd) = (shape.linear_k_dim(), shape.linear_v_dim(), shape.linear_head_dim);
                let width = 2 * dk + dv;
                artifact = artifact
                    .with_tensor(n("linear_q.weight"), weights(dk * d))
                    .with_tensor(n("linear_k.weight"), weights(dk * d))
                    .with_tensor(n("linear_v.weight"), weights(dv * d))
                    .with_tensor(n("linear_z.weight"), weights(dv * d))
                    .with_tensor(n("linear_conv.weight"), weights(shape.conv_kernel * width))
                    .with_tensor(n("linear_dt.weight"), weights(shape.linear_v_heads * d))
                    .with_tensor(n("linear_beta.weight"), weights(shape.linear_v_heads * d))
                    .with_tensor(n("linear_o.weight"), weights(d * dv))
                    .with_params(n("linear_q.weight.a16"), &[projection(d)])
                    .with_params(n("linear_k.weight.a16"), &[projection(d)])
                    .with_params(n("linear_v.weight.a16"), &[projection(d)])
                    .with_params(n("linear_z.weight.a16"), &[projection(d)])
                    // The convolution reduces over four taps, so it barely attenuates.
                    .with_params(n("linear_conv.a16"), &[A16QuantParams { multiplier: 1, shift: 16, zero: 0 }])
                    .with_params(n("linear_conv_act.a16"), &[unity])
                    .with_params(n("linear_dt.weight.a16"), &[projection(d)])
                    .with_params(n("linear_beta.weight.a16"), &[projection(d)])
                    // A coefficient of ONE is `exp(−softplus(dt))`; the bias is zero so the
                    // fixture's decay is a function of the projection alone.
                    .with_params(
                        n("linear_decay_c.a16"),
                        &[A16QuantParams { multiplier: 1, shift: 0, zero: kaspa_consensus_core::palw_base0::ONE }],
                    )
                    .with_params(n("linear_dt_bias.a16"), &[A16QuantParams { multiplier: 1, shift: 0, zero: 0 }])
                    // The state carries eight bits above the value scale.
                    .with_params(n("linear_read.a16"), &[A16QuantParams { multiplier: 1, shift: 23, zero: 0 }])
                    .with_params(n("linear_delta.a16"), &[A16QuantParams { multiplier: 1, shift: 0, zero: 0 }])
                    .with_params(n("linear_write.a16"), &[A16QuantParams { multiplier: 1, shift: 7, zero: 0 }])
                    .with_params(n("linear_out.a16"), &[A16QuantParams { multiplier: 1, shift: 23, zero: 0 }])
                    .with_params(n("linear_norm.a16"), &[unity])
                    .with_params(n("linear_norm_eps.a16"), &[A16QuantParams { multiplier: 1, shift: 0, zero: 0 }])
                    .with_params(n("linear_gate.a16"), &[unity])
                    .with_params(n("linear_gated.a16"), &[A16QuantParams { multiplier: 1, shift: 15, zero: 0 }])
                    .with_params(n("linear_o.weight.a16"), &[projection(dv)]);
                let _ = hd;
            }
            Qwen36LayerKind::FullAttention => {
                let (q_dim, kv_dim) = (shape.n_heads * shape.head_dim, shape.kv_dim());
                if shape.attn_output_gate() {
                    artifact = artifact
                        .with_tensor(n("attn_gate.weight"), weights(q_dim * d))
                        .with_params(n("attn_gate.weight.a16"), &[projection(d)])
                        .with_params(n("attn_gated.a16"), &[A16QuantParams { multiplier: 1, shift: 24, zero: 0 }]);
                }
                artifact = artifact
                    .with_tensor(n("attn_q.weight"), weights(q_dim * d))
                    .with_tensor(n("attn_k.weight"), weights(kv_dim * d))
                    .with_tensor(n("attn_v.weight"), weights(kv_dim * d))
                    .with_tensor(n("attn_o.weight"), weights(d * q_dim))
                    .with_params(n("attn_q.weight.a16"), &[projection(d)])
                    .with_params(n("attn_k.weight.a16"), &[projection(d)])
                    .with_params(n("attn_v.weight.a16"), &[projection(d)])
                    .with_params(n("attn_q_norm.a16"), &[unity])
                    .with_params(n("attn_k_norm.a16"), &[unity])
                    .with_params(n("attn_rope.a16"), &[A16QuantParams { multiplier: 1, shift: 24, zero: 0 }])
                    .with_params(n("attn_logits.a16"), &[projection(shape.head_dim)])
                    .with_params(n("attn_softmax_up.a16"), &[A16QuantParams { multiplier: 1, shift: 0, zero: 16 }])
                    .with_params(n("attn_probs.a16"), &[A16QuantParams { multiplier: 1, shift: 9, zero: 0 }])
                    .with_params(n("attn_values.a16"), &[A16QuantParams { multiplier: 1, shift: 15, zero: 0 }])
                    .with_params(n("attn_o.weight.a16"), &[projection(q_dim)]);
            }
        }
        // The mixture: a router, every expert — and the shared one where the shape has it.
        artifact = artifact
            .with_tensor(n("ffn_router.weight"), weights(shape.n_experts * d))
            .with_params(n("ffn_router.weight.a16"), &[projection(d)])
            .with_params(n("ffn_router.a16"), &[unity])
            // The router's softmax widening, class data per layer rather than a shape constant.
            .with_params(n("ffn_router_up.a16"), &[A16QuantParams { multiplier: 1, shift: 0, zero: 20 }])
            .with_params(n("ffn_combine.a16"), &[A16QuantParams { multiplier: 1, shift: 24, zero: 0 }])
            .with_params(n("ffn_moe_out.a16"), &[unity]);
        if shape.has_shared_expert() {
            artifact = artifact
                .with_tensor(n("ffn_shared_gate.weight"), weights(d))
                .with_params(n("ffn_shared_gate.weight.a16"), &[projection(d)])
                .with_params(n("ffn_shared_gated.a16"), &[A16QuantParams { multiplier: 1, shift: 24, zero: 0 }]);
        }
        let shared_tail: Vec<(String, usize)> =
            if shape.has_shared_expert() { vec![(format!("blk.{li}.ffn_shared_expert"), shape.shared_dim)] } else { Vec::new() };
        for (base, mid) in (0..shape.n_experts).map(|e| (format!("blk.{li}.ffn_expert.{e}"), shape.moe_dim)).chain(shared_tail) {
            artifact = artifact
                .with_tensor(format!("{base}_gate.weight"), weights(mid * d))
                .with_tensor(format!("{base}_up.weight"), weights(mid * d))
                .with_tensor(format!("{base}_down.weight"), weights(d * mid))
                .with_params(format!("{base}_gate.weight.a16"), &[projection(d)])
                .with_params(format!("{base}_up.weight.a16"), &[projection(d)])
                .with_params(format!("{base}_silu.a16"), &[unity])
                .with_params(format!("{base}_gated.a16"), &[A16QuantParams { multiplier: 1, shift: 15, zero: 0 }])
                .with_params(format!("{base}_down.weight.a16"), &[projection(mid)]);
        }
    }
    artifact
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Qwen3.6-SHAPED artifact at a size a test can run: the same layer alternation, the same
    /// two arms, a real router over a real expert count, and everything else cut down.
    ///
    /// Not a calibration — the triples are derived from each site's fan-in, the same rule the
    /// dense tier's fixture uses — and the weights are an LCG. What it proves is that the graph
    /// composes and produces a non-degenerate row, which is the question at this stage. Fidelity
    /// is the converter's question and needs a checkpoint.
    pub(crate) fn fixture(layers: usize, experts: usize) -> Qwen36ArtifactV1 {
        super::fixture_impl(layers, experts)
    }

    /// **The graph composes.** Both arms, the mixture, the residual stream, forty-style layer
    /// alternation — end to end, with a row out that is neither zero nor constant.
    /// **The qwen3moe flavor runs end to end** — gateless attention, no shared expert, no
    /// recurrence anywhere — through the same engine the hybrid uses. The cheap in-crate proof
    /// that the 30B conversion is worth starting.
    #[test]
    fn the_qwen3moe_graph_runs_end_to_end() {
        let artifact = super::qwen3moe_dev_fixture(3, 8);
        assert!(artifact.shape.is_full_attention_only() && !artifact.shape.attn_output_gate());
        let engine = Qwen36Engine::new(&artifact);
        let mut cache = Qwen36Cache::new(&artifact.shape);
        let mut rows = Vec::new();
        for position in 0..6 {
            let token = (position * 7 + 3) % artifact.shape.vocab;
            rows.push(engine.forward_token(&mut cache, token, position).expect("the gateless pass completes"));
        }
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(row.len(), artifact.shape.vocab);
            assert!(row.iter().any(|v| *v != 0), "position {i} produced an all-zero logit row");
        }
        assert_ne!(rows[0], rows[1]);
        // And determinism, which is the property the class registers.
        let rerun = {
            let mut cache = Qwen36Cache::new(&artifact.shape);
            let mut last = Vec::new();
            for position in 0..6 {
                last = engine.forward_token(&mut cache, (position * 7 + 3) % artifact.shape.vocab, position).expect("completes");
            }
            last
        };
        assert_eq!(rows[5], rerun);
    }

    #[test]
    fn the_hybrid_graph_runs_end_to_end() {
        let artifact = fixture(8, 16);
        let engine = Qwen36Engine::new(&artifact);
        let mut cache = Qwen36Cache::new(&artifact.shape);

        let mut rows = Vec::new();
        for position in 0..6 {
            let token = (position * 7 + 3) % artifact.shape.vocab;
            rows.push(engine.forward_token(&mut cache, token, position).expect("the pass completes"));
        }
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(row.len(), artifact.shape.vocab);
            assert!(row.iter().any(|v| *v != 0), "position {i} produced an all-zero logit row");
            let distinct: std::collections::BTreeSet<i32> = row.iter().copied().collect();
            assert!(distinct.len() > 1, "position {i} produced a constant logit row");
        }
        // Different tokens at different positions must move the row, or nothing was computed.
        assert_ne!(rows[0], rows[1]);
        assert_ne!(rows[1], rows[2]);
    }

    /// The property the class exists for: same input, same bits — including the recurrent state,
    /// which is the one thing in this architecture that could carry a difference forward.
    #[test]
    fn the_hybrid_pass_is_deterministic() {
        let artifact = fixture(8, 16);
        let engine = Qwen36Engine::new(&artifact);
        let run = || {
            let mut cache = Qwen36Cache::new(&artifact.shape);
            let mut last = Vec::new();
            for position in 0..8 {
                last = engine.forward_token(&mut cache, (position * 5 + 1) % 64, position).expect("completes");
            }
            let states: Vec<Vec<i32>> = cache.gdn.iter().flatten().map(|s| s.s.clone()).collect();
            (last, states)
        };
        assert_eq!(run(), run());
    }

    /// The recurrent state must actually be carrying history: a GatedDeltaNet layer whose state
    /// stayed at zero would still produce a plausible row through the residual path.
    #[test]
    fn the_recurrent_state_fills() {
        let artifact = fixture(8, 16);
        let engine = Qwen36Engine::new(&artifact);
        let mut cache = Qwen36Cache::new(&artifact.shape);
        for position in 0..6 {
            engine.forward_token(&mut cache, position % 64, position).expect("completes");
        }
        let mut filled = 0usize;
        for layer in &cache.gdn {
            for state in layer {
                if state.s.iter().any(|v| *v != 0) {
                    filled += 1;
                }
            }
        }
        assert!(filled > 0, "no GatedDeltaNet head accumulated any state");
        // And the softmax layers kept a cache.
        assert_eq!(cache.len(), 6);
    }

    /// The same position must see the same rotation whichever arm asks for it, and a position past
    /// the table is refused rather than wrapped.
    #[test]
    fn a_position_past_the_table_is_refused() {
        let artifact = fixture(4, 8);
        let engine = Qwen36Engine::new(&artifact);
        let mut cache = Qwen36Cache::new(&artifact.shape);
        assert_eq!(engine.forward_token(&mut cache, 1, artifact.shape.max_position), Err(Qwen36Error::Position));
        assert_eq!(engine.forward_token(&mut cache, artifact.shape.vocab, 0), Err(Qwen36Error::Position));
    }

    /// **The store is keyed by strings, so two rows must not be able to claim one key.**
    ///
    /// The shared expert's gate projection and the mixture's scalar gate both wanted
    /// `blk.N.ffn_shared_gate.weight`. Nothing in a `BTreeMap` notices that; the engine caught it
    /// only because the two rows happen to have different widths and it checks sizes. Two rows of
    /// the same width would have run to completion and computed something else, which is the
    /// failure mode this test exists to keep closed.
    #[test]
    fn no_two_rows_claim_one_key() {
        let artifact = fixture(4, 8);
        // Every name the engine can ask for, built the way the engine builds them.
        let mut asked: Vec<String> = Vec::new();
        for li in 0..artifact.shape.n_layers() {
            asked.push(format!("blk.{li}.ffn_shared_gate.weight"));
            for suffix in ["_gate.weight", "_up.weight", "_down.weight"] {
                asked.push(format!("blk.{li}.ffn_shared_expert{suffix}"));
                for e in 0..artifact.shape.n_experts {
                    asked.push(format!("blk.{li}.ffn_expert.{e}{suffix}"));
                }
            }
        }
        let unique: std::collections::BTreeSet<&String> = asked.iter().collect();
        assert_eq!(unique.len(), asked.len(), "two different rows resolve to one store key");
        // And every one of them is actually present, so the check is over real keys rather than
        // over names nothing reads.
        for name in &asked {
            assert!(artifact.tensor(name).is_ok(), "the fixture is missing {name}");
        }
    }

    /// **The artifact file round-trips, and a mapped artifact runs.**
    ///
    /// Not "the bytes match" — the ENGINE must produce the same logits from the mapped artifact as
    /// from the in-memory one, because the mapped path is the only one a 33 GiB class can use and
    /// a difference there would be invisible until a court disagreed with a producer.
    #[test]
    fn a_mapped_artifact_runs_identically_to_an_owned_one() {
        let owned = fixture(4, 8);
        let path = std::env::temp_dir().join(format!("misaka-q36-{}.palwq36", std::process::id()));

        let plan: Vec<(String, usize)> =
            owned.tensor_names().iter().map(|n| (n.to_string(), owned.tensor(n).expect("present").len())).collect();
        let mut writer =
            Qwen36Writer::create(&path, &owned.shape, &owned.rope, owned.params_map(), plan.clone()).expect("the file is created");
        for (name, _) in &plan {
            writer.push(name, &owned.tensor(name).expect("present")).expect("the tensor is appended");
        }
        let written = writer.finish().expect("the plan is filled");
        assert_eq!(written, owned.weight_bytes());

        let mapped = open_artifact(&path).expect("the artifact opens");
        assert_eq!(mapped.shape, owned.shape);
        assert_eq!(mapped.rope.cos_q, owned.rope.cos_q);
        assert_eq!(mapped.weight_bytes(), owned.weight_bytes());
        for name in owned.tensor_names() {
            assert_eq!(mapped.tensor(name).expect("mapped"), owned.tensor(name).expect("owned"), "tensor {name}");
        }

        let run = |a: &Qwen36ArtifactV1| {
            let engine = Qwen36Engine::new(a);
            let mut cache = Qwen36Cache::new(&a.shape);
            let mut last = Vec::new();
            for position in 0..5 {
                last = engine.forward_token(&mut cache, (position * 7 + 3) % a.shape.vocab, position).expect("completes");
            }
            last
        };
        assert_eq!(run(&mapped), run(&owned), "a mapped artifact must compute what an owned one computes");
        std::fs::remove_file(&path).ok();
    }

    /// **Parameters are patched in place after the pass that measured them.**
    ///
    /// The weights can be quantized without a calibration — the codes are a per-output-channel
    /// property of the weight and nothing else — but a scale is a statement about a RANGE, and a
    /// range has to be observed by running the model. One pass therefore writes the codes and
    /// measures the ranges, and the triples are rewritten at the end. Only the seventeen bytes of
    /// each triple move; the directory does not.
    #[test]
    fn parameters_can_be_rewritten_after_the_weights_are_written() {
        let owned = fixture(2, 8);
        let path = std::env::temp_dir().join(format!("misaka-q36-patch-{}.palwq36", std::process::id()));
        let plan: Vec<(String, usize)> =
            owned.tensor_names().iter().map(|n| (n.to_string(), owned.tensor(n).expect("present").len())).collect();
        let mut writer = Qwen36Writer::create(&path, &owned.shape, &owned.rope, owned.params_map(), plan.clone()).expect("created");
        for (name, _) in &plan {
            writer.push(name, &owned.tensor(name).expect("present")).expect("appended");
        }

        // A measured value for one row, the same width as the placeholder.
        let target = "blk.0.attn_norm.a16".to_string();
        let measured = A16QuantParams { multiplier: 12_345, shift: 9, zero: -7 };
        let mut patch = BTreeMap::new();
        patch.insert(target.clone(), measured.to_wire().to_vec());
        writer.finish_with_params(&patch).expect("the patch lands");

        let mapped = open_artifact(&path).expect("opens");
        assert_eq!(mapped.param_rows(&target).expect("present"), vec![measured]);
        // Everything else is untouched, weights included.
        for name in owned.tensor_names() {
            assert_eq!(mapped.tensor(name).expect("mapped"), owned.tensor(name).expect("owned"), "tensor {name}");
        }
        assert_eq!(
            mapped.param_rows("blk.0.attn_align.a16").expect("present"),
            owned.param_rows("blk.0.attn_align.a16").expect("present")
        );
        std::fs::remove_file(&path).ok();
    }

    /// **The root does not depend on the access path.** The streaming pass over a mapped file
    /// and the in-memory walk over the same content are one digest — which is what licenses the
    /// root pass to read through the file descriptor instead of faulting the map. The hash is a
    /// stream, so chunking is invisible; this pins that against both stores.
    #[test]
    fn the_mapped_root_equals_the_owned_root() {
        let owned = fixture(2, 8);
        let path = std::env::temp_dir().join(format!("misaka-q36-rootpath-{}.palwq36", std::process::id()));
        let plan: Vec<(String, usize)> =
            owned.tensor_names().iter().map(|n| (n.to_string(), owned.tensor(n).expect("present").len())).collect();
        let mut writer = Qwen36Writer::create(&path, &owned.shape, &owned.rope, owned.params_map(), plan.clone()).expect("created");
        for (name, _) in &plan {
            writer.push(name, &owned.tensor(name).expect("present")).expect("appended");
        }
        writer.finish().expect("closed");
        let mapped = open_artifact(&path).expect("opens");
        assert!(matches!(mapped.store, Store::Mapped { .. }), "the reopened artifact is the mapped store");
        assert_eq!(mapped.artifact_root(), owned.artifact_root(), "one digest, two access paths");
        std::fs::remove_file(&path).ok();
    }

    /// A patch the header did not declare, or one of a different width, is an error. A row the
    /// header does not list is a row no reader will look for, and a row that changed width would
    /// move every row after it.
    #[test]
    fn a_patch_the_header_did_not_declare_is_refused() {
        let owned = fixture(1, 8);
        let path = std::env::temp_dir().join(format!("misaka-q36-badpatch-{}.palwq36", std::process::id()));
        let plan: Vec<(String, usize)> =
            owned.tensor_names().iter().map(|n| (n.to_string(), owned.tensor(n).expect("present").len())).collect();
        let mut writer = Qwen36Writer::create(&path, &owned.shape, &owned.rope, owned.params_map(), plan.clone()).expect("created");
        for (name, _) in &plan {
            writer.push(name, &owned.tensor(name).expect("present")).expect("appended");
        }
        let mut patch = BTreeMap::new();
        patch.insert("blk.0.not_a_row.a16".to_string(), vec![0u8; A16QuantParams::WIRE_BYTES]);
        assert!(writer.finish_with_params(&patch).is_err());
        std::fs::remove_file(&path).ok();
    }

    /// The writer refuses a tensor the plan did not declare. The directory is already on disk when
    /// the first tensor arrives, so a tensor out of order would be silently mis-addressed by every
    /// reader — there is no later moment at which that could be noticed.
    #[test]
    fn the_writer_refuses_a_tensor_the_plan_did_not_declare() {
        let owned = fixture(1, 8);
        let path = std::env::temp_dir().join(format!("misaka-q36-plan-{}.palwq36", std::process::id()));
        let names = owned.tensor_names();
        let plan: Vec<(String, usize)> = names.iter().map(|n| (n.to_string(), owned.tensor(n).expect("present").len())).collect();

        let mut writer = Qwen36Writer::create(&path, &owned.shape, &owned.rope, owned.params_map(), plan.clone()).expect("created");
        // Out of order.
        assert!(writer.push(&plan[1].0, &owned.tensor(&plan[1].0).expect("present")).is_err());
        // Right name, wrong length.
        assert!(writer.push(&plan[0].0, &[0i8]).is_err());
        // And a file that is not filled does not close.
        assert!(writer.finish().is_err());
        std::fs::remove_file(&path).ok();
    }

    /// A missing tensor names itself. The store is the whole registration surface, so a class that
    /// is missing a row has to say which one.
    #[test]
    fn a_missing_row_names_itself() {
        // A fresh fixture rather than a clone: a mapped artifact cannot be cloned (the map is a
        // resource, not a value), so the type is not `Clone` and this test builds what it strips.
        let mut stripped = fixture(4, 8);
        match &mut stripped.store {
            Store::Owned(t) => {
                t.remove("blk.0.linear_q.weight");
            }
            Store::Mapped { .. } => unreachable!("the fixture is owned"),
        }
        let engine = Qwen36Engine::new(&stripped);
        let mut cache = Qwen36Cache::new(&stripped.shape);
        assert_eq!(engine.forward_token(&mut cache, 1, 0), Err(Qwen36Error::MissingTensor("blk.0.linear_q.weight".to_string())));
    }

    /// Write an owned fixture to a `.palwq36` in the temp dir, the way the round-trip test does.
    fn written(owned: &Qwen36ArtifactV1, tag: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("misaka-q36-{tag}-{}.palwq36", std::process::id()));
        let plan: Vec<(String, usize)> =
            owned.tensor_names().iter().map(|n| (n.to_string(), owned.tensor(n).expect("present").len())).collect();
        let mut writer = Qwen36Writer::create(&path, &owned.shape, &owned.rope, owned.params_map(), plan.clone()).expect("created");
        for (name, _) in &plan {
            writer.push(name, &owned.tensor(name).expect("present")).expect("appended");
        }
        writer.finish().expect("closed");
        path
    }

    /// How many positions the residency tests walk: enough for a fifth of the fixture's experts
    /// to be exhausted and evicted, inside the fixture's 32-position rotary table.
    const RUN_POSITIONS: usize = 24;

    /// `RUN_POSITIONS` positions through an artifact, the last logit row returned; the
    /// residency's numbers after every token handed to `check`, so a test can hold the budget at
    /// every step rather than only at the end.
    fn run_checked(a: &Qwen36ArtifactV1, mut check: impl FnMut(usize, Option<Qwen36ResidencyStatsV1>)) -> Vec<i32> {
        let engine = Qwen36Engine::new(a);
        let mut cache = Qwen36Cache::new(&a.shape);
        let mut last = Vec::new();
        for position in 0..RUN_POSITIONS {
            last = engine.forward_token(&mut cache, (position * 7 + 3) % a.shape.vocab, position).expect("completes");
            check(position, a.residency_stats());
        }
        last
    }

    /// **ADR-0112's ratio: an artifact held within a fifth of its weight bytes computes what an
    /// owned one computes, token for token, and never holds more than its budget.** The fixture
    /// routes 4 of 256 experts a layer over 4 layers (the class's own count; at the fixture's
    /// width the always-set is a larger share than the class's, so fewer experts would put a
    /// fifth under the floor). A fifth of it holds the always-set and a sixth of the experts, and
    /// twenty-four tokens miss and evict throughout. The floor — the tightest budget the class
    /// runs in — computes the same rows with more evictions, and the page cache path, which is
    /// the path before ADR-0112, still does too.
    #[test]
    fn a_budgeted_artifact_computes_what_an_owned_one_does_at_a_fifth_of_its_size() {
        let owned = fixture(4, 256);
        let path = written(&owned, "fifth");
        let expected = run_checked(&owned, |_, stats| assert!(stats.is_none(), "an owned store has no residency"));

        let fifth = open_artifact_with_residency(&path, Qwen36ResidencyPolicyV1::FifthOfTheWeights).expect("opens");
        let opened = fifth.residency_stats().expect("budgeted");
        assert_eq!(opened.budget_bytes, (owned.weight_bytes() as u64).div_ceil(QWEN36_RESIDENT_FRACTION_DENOMINATOR_V1));
        let floor = fifth.residency_floor_bytes().expect("a floor");
        assert!(opened.budget_bytes >= floor, "a fifth of this fixture ({}) is above its floor ({floor})", opened.budget_bytes);
        assert!(opened.pinned_bytes > 0 && opened.bytes_read == opened.pinned_bytes, "the always-set was read at open: {opened:?}");
        let got = run_checked(&fifth, |position, stats| {
            let s = stats.expect("budgeted");
            assert!(s.resident_expert_bytes <= s.expert_budget_bytes(), "position {position}: {s:?} is over its budget");
        });
        assert_eq!(got, expected, "a budgeted artifact must compute what an owned one computes");
        let s = fifth.residency_stats().expect("budgeted");
        assert!(s.misses > 0, "the experts the router chose were read on their first use: {s:?}");
        assert!(s.hits + s.misses >= (RUN_POSITIONS * 4 * 4) as u64, "every routed expert of every token was looked up: {s:?}");
        assert!(s.bytes_read > s.pinned_bytes, "the misses were read from the file: {s:?}");

        // The floor: the tightest budget the class runs in holds at most one token's experts, and
        // still computes the rows.
        let tight = open_artifact_with_residency(&path, Qwen36ResidencyPolicyV1::Bytes(floor)).expect("the floor opens");
        assert_eq!(
            run_checked(&tight, |position, stats| {
                let s = stats.expect("budgeted");
                assert!(
                    s.resident_expert_bytes <= s.token_expert_bytes,
                    "position {position}: the floor holds one token's experts: {s:?}"
                );
            }),
            expected,
            "at the floor too"
        );

        let paged = open_artifact(&path).expect("opens");
        assert!(paged.residency_stats().is_none(), "the page cache path has no loader");
        assert_eq!(run_checked(&paged, |_, _| {}), expected, "and computes the same rows");
        std::fs::remove_file(&path).ok();
    }

    /// A budget below the floor is refused at open, with the floor and both its terms in the
    /// message — never opened and left to re-read what it just read.
    #[test]
    fn a_budget_below_the_floor_is_refused_by_name() {
        let owned = fixture(2, 256);
        let path = written(&owned, "floor");
        let floor = open_artifact_with_residency(&path, Qwen36ResidencyPolicyV1::FifthOfTheWeights)
            .expect("opens")
            .residency_floor_bytes()
            .expect("a floor");
        match open_artifact_with_residency(&path, Qwen36ResidencyPolicyV1::Bytes(floor - 1)) {
            Err(Qwen36Error::Residency(why)) => {
                assert!(why.contains("floor") && why.contains(&floor.to_string()) && why.contains("always-set"), "{why}");
            }
            other => panic!("a budget below the floor must be refused by name, got {:?}", other.map(|_| ())),
        }
        assert!(open_artifact_with_residency(&path, Qwen36ResidencyPolicyV1::Bytes(floor)).is_ok(), "the floor itself opens");
        std::fs::remove_file(&path).ok();
    }

    /// Under a budget: the embedding row read directly is the table's row, the root does not
    /// depend on where the expert parameter rows live, and an expert read outside an admission
    /// is admitted on the way — its parameter row a hit on the same holding.
    #[test]
    fn the_budgeted_paths_read_what_the_owned_ones_hold() {
        let owned = fixture(2, 256);
        let path = written(&owned, "paths");
        let budgeted = open_artifact_with_residency(&path, Qwen36ResidencyPolicyV1::FifthOfTheWeights).expect("opens");
        let d = owned.shape.d_model;
        for token in [0usize, 1, 17, owned.shape.vocab - 1] {
            assert_eq!(budgeted.embedding_row(token, d).unwrap(), owned.embedding_row(token, d).unwrap(), "row {token}");
        }
        assert_eq!(budgeted.artifact_root(), owned.artifact_root(), "one digest, whether the expert rows are owned or in the file");
        assert_eq!(budgeted.residency_stats().unwrap().misses, 0, "nothing is admitted by a root pass or a row read");
        let gate = "blk.1.ffn_expert.5_gate.weight";
        assert_eq!(budgeted.tensor(gate).unwrap(), owned.tensor(gate).unwrap());
        let s = budgeted.residency_stats().unwrap();
        assert_eq!((s.misses, s.hits), (1, 0), "one miss admitted the expert: {s:?}");
        assert_eq!(
            budgeted.param_rows("blk.1.ffn_expert.5_silu.a16").unwrap(),
            owned.param_rows("blk.1.ffn_expert.5_silu.a16").unwrap()
        );
        assert_eq!(budgeted.residency_stats().unwrap().hits, 1, "its parameter row is a hit on the same holding");
        // Every tensor, whichever tier holds it — which walks all 512 routed experts through a
        // budget that holds about 80 of them: the churn the run above cannot promise, because
        // where a fixture's router goes is the fixture's business.
        let before = budgeted.residency_stats().unwrap();
        for name in owned.tensor_names() {
            assert_eq!(budgeted.tensor(name).unwrap(), owned.tensor(name).unwrap(), "tensor {name}, whichever tier holds it");
            let s = budgeted.residency_stats().unwrap();
            assert!(s.resident_expert_bytes <= s.expert_budget_bytes(), "the walk never holds more than the budget: {s:?}");
        }
        let after = budgeted.residency_stats().unwrap();
        let experts = 2 * 256;
        let capacity = after.expert_budget_bytes() / after.token_expert_bytes.max(1) * 4 * 2;
        assert!(after.misses - before.misses >= experts as u64 - 1, "every expert not already held was read: {after:?}");
        assert!(after.evictions >= experts as u64 - capacity - 1, "what the budget could not hold was given back: {after:?}");
        for name in owned.params_map().keys() {
            assert_eq!(budgeted.param_rows(name).unwrap(), owned.param_rows(name).unwrap(), "row {name}");
        }
        // And after all that churn the forward pass still computes the owned store's rows.
        assert_eq!(run_checked(&budgeted, |_, _| {}), run_checked(&owned, |_, _| {}), "the rows after the churn");
        std::fs::remove_file(&path).ok();
    }

    /// The policy's arithmetic: a fifth rounds up, bytes are bytes, the page cache is none.
    #[test]
    fn the_residency_policy_arithmetic() {
        assert_eq!(Qwen36ResidencyPolicyV1::FifthOfTheWeights.budget_for(35_727_649_280), Some(7_145_529_856));
        assert_eq!(Qwen36ResidencyPolicyV1::FifthOfTheWeights.budget_for(11), Some(3));
        assert_eq!(Qwen36ResidencyPolicyV1::Bytes(7).budget_for(100), Some(7));
        assert_eq!(Qwen36ResidencyPolicyV1::PageCache.budget_for(100), None);
        assert_eq!(expert_key_v1("blk.12.ffn_expert.255_down.weight.exp"), Some(((12, 255), "down.weight.exp")));
        assert_eq!(expert_key_v1("blk.3.ffn_expert.7_silu.a16"), Some(((3, 7), "silu.a16")));
        assert_eq!(expert_key_v1("blk.3.ffn_shared_expert_gate.weight"), None, "the shared expert is pinned");
        assert_eq!(expert_key_v1("token_embd.weight"), None);
    }
}
