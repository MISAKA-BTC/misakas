//! **ADR-0096 Decisions 6–8, built: the constrained free-prompt job (version 6) and its mask** — RFC-0001 §2.5.
//!
//! A constrained job is a V3 job (greedy, no decode rules) whose committed token at each position is the argmax over the
//! lanes the job's decode constraint ADMITS from the automaton state the committed prefix reached
//! ([`decode_token_select_v3`]); when no lane is admitted the committed token is the class's lowest end-of-generation id
//! and the run stops there. The constraint's canonical bytes ride the job's tail (so they are inside `fp_job_id`, the
//! claim id and the signature — a constraint cannot change after the fact), and its id is
//! [`constraint_id_v1`]`(bytes)`. The bytes are as public as the prompt ids on the network the job runs on (PublicDa),
//! which is ADR-0096 Decision 7's own sentence; that they ride the commitment's job rather than a served envelope is this
//! build's, so a seat reads them from the same payload that names the claim.
//!
//! **What the host owes.** The mask needs the class's token-to-bytes table ([`PalwTokenTableV1`]: the rendering of every
//! id, and the end-of-generation ids). The table is served material named by the job's `tokenizer_id`: a host builds a
//! [`PalwConstraintMaskV1`] only when the table was derived under the job's `tokenizer_id` (`PalwTokenTableV1::tokenizer_id`), and opens it as a scope around the run
//! ([`palw_fp_with_constraint_scope_v1`]) — a producer's capture and a seat's replay both. A host without the table files
//! `Incapable` (an abstention, ADR-0065 D4), never a verdict.
//!
//! **What the chain does.** Behind `Params::palw_fp_decode_constraint` the doors admit version 6 (isolation by the
//! ruleset's fence, height by the header-context door), the extraction walk judges it on its V3 stand-in and the fold
//! prices it as the V3 job it is; a version-2 constraint form is admitted only past `palw_fp_constraint_v2`
//! ([`crate::palw_fp_constraint_v2`]). The court's third arm — "the committed token is not admitted" — is
//! `check_tiled_decode_token_refutation_v3` (`palw_step_refute`), which reads the same rule.

use crate::Hash64;
use crate::palw_decode_constraint_v1::{
    PalwConstraintStateV1, PalwDecodeConstraintV1, constraint_admits_lane_v1, constraint_id_v1, constraint_state_after_v1,
};
use crate::palw_freeprompt_v3::{PalwFpCommitmentTxPayloadV3, PalwFpDecodeRulesV1, PalwFpJobTailV1, PalwFpV3Error, PalwFreePromptJobV3};
use crate::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use std::sync::Arc;

/// **The entry a drill arms `palw_fp_decode_constraint` with** (`--palw-drill-fp-constraint-at`,
/// [`crate::config::drill`]). In NO testnet-12 flag-day list: dormant on every network.
pub const PALW_DRILL_FP_DECODE_CONSTRAINT_ENTRY: crate::config::params::PalwPostLaunchFenceV1 =
    crate::config::params::PalwPostLaunchFenceV1 {
        name: "palw_fp_decode_constraint",
        set: |params, at| {
            params.palw_fp_decode_constraint = at;
            params.sync_palw_fp_decode_constraint_v1();
        },
    };

impl crate::config::params::Params {
    /// **The fence's mirror** on the V2 bundle's state params (`fp_decode_constraint_from_daa`), which the fold reads.
    /// `None` where the fence is not armed (or is `never()`).
    pub fn sync_palw_fp_decode_constraint_v1(&mut self) {
        let from_daa = self
            .palw_fp_decode_constraint
            .filter(|f| *f != crate::config::params::ForkActivation::never())
            .map(|f| f.daa_score());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_fp_decode_constraint_from_daa(from_daa);
        }
    }
}

/// The drill's one-entry list.
pub const PALW_DRILL_FP_DECODE_CONSTRAINT_FENCES_V1: &[crate::config::params::PalwPostLaunchFenceV1] = &[PALW_DRILL_FP_DECODE_CONSTRAINT_ENTRY];

/// **The constrained job's version**: 6 — the number ADR-0096 Decision 8 reserved (7 is V4, 8 V5, 9 an evaluation job, 10 a
/// tensor job, 11 a prefix-state job).
pub const PALW_FP_CONSTRAINT_VERSION: u16 = 6;
/// The constrained job id's key: a domain no other job's id uses.
pub const PALW_FP_CONSTRAINT_DOMAIN_JOB_ID: &[u8] = b"misaka-palw/fp-constraint/job-id/v1";
/// The key of a class token table's digest.
pub const PALW_TOKEN_TABLE_DOMAIN_V1: &[u8] = b"misaka-palw/token-table/v1";

/// **A constrained job's tail**: the decode constraint's canonical bytes and the root of the class token table the job's
/// mask is read through ([`PalwTokenTableV1::root`]). The root is the job's own statement of which table it was produced
/// under — a seat that holds the class's real table refuses a job naming another (`Unverifiable`), and the court opens the
/// renderings it needs against THIS root, so a producer cannot put a rendering in front of the court that its own claim did not
/// commit to.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwFpConstraintTailV1 {
    pub constraint: Vec<u8>,
    pub table_root: Hash64,
}

/// **`fp_job_id_constraint_v1`**: the whole borsh of the job (every V3 field, then the constraint's bytes) under this
/// version's key — the V3/V4 ids' construction.
pub fn fp_job_id_constraint_v1(job: &PalwFreePromptJobV3) -> Hash64 {
    let bytes = borsh::to_vec(job).expect("a free-prompt job is borsh-serializable");
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_FP_CONSTRAINT_DOMAIN_JOB_ID).to_state();
    state.update(&(bytes.len() as u64).to_le_bytes());
    state.update(&bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

// ---------------------------------------------------------------------------------------------
// The class's token table
// ---------------------------------------------------------------------------------------------

/// **A class's token-to-bytes table** (ADR-0096 Decision 8's served material): the byte rendering of every id (`None` for
/// an id the table cannot render, which is never admitted) and the class's end-of-generation ids (the rule commits the
/// LOWEST of them when no lane is admitted; they render no bytes of their own).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTokenTableV1 {
    pub entries: Vec<Option<Vec<u8>>>,
    pub eog_token_ids: Vec<u32>,
    /// The `tokenizer_id` this table was derived under — what a host asserts of it (a worker derives the table from the
    /// tokenizer file it checked against the class's `tokenizer_id`). A job's mask is built only when the job names the same
    /// one. [`Self::digest`] does not cover it, so a table with no other commitment can name its own digest.
    pub tokenizer_id: Hash64,
}

impl PalwTokenTableV1 {
    pub fn vocab(&self) -> u32 {
        self.entries.len() as u32
    }

    /// The rendering the mask reads: `Some(bytes)` for a non-empty rendering of an id that is not an end-of-generation id.
    pub fn rendering(&self, id: u32) -> Option<&[u8]> {
        if self.eog_token_ids.contains(&id) {
            return None;
        }
        self.entries.get(id as usize)?.as_deref().filter(|bytes| !bytes.is_empty())
    }

    pub fn lowest_eog_id(&self) -> Option<u32> {
        self.eog_token_ids.iter().copied().min()
    }

    /// The table's Merkle leaves: one per id (a presence byte, the id, the rendering), then one for the sorted
    /// end-of-generation ids — index `vocab`.
    pub fn leaf_hashes(&self) -> Vec<Hash64> {
        let mut leaves: Vec<Hash64> = self
            .entries
            .iter()
            .enumerate()
            .map(|(id, entry)| table_leaf_hash_v1(id as u32, entry.as_deref()))
            .collect();
        leaves.push(table_eog_leaf_hash_v1(&self.eog_token_ids));
        leaves
    }

    /// **The table's root** — the Merkle root of [`Self::leaf_hashes`] (the step tree's fold, so one fold spells every tree
    /// the court opens). What a constrained job's tail names and a seat and the court hold the table to.
    pub fn root(&self) -> Hash64 {
        crate::palw_step_leg::step_merkle_root_v1(&self.leaf_hashes()).expect("a token table has between 1 and 2^25 - 1 ids")
    }

    /// The opening of one id's rendering against [`Self::root`].
    pub fn opening_of(&self, id: u32) -> Option<PalwTokenTableOpeningV1> {
        let entry = self.entries.get(id as usize)?;
        let opening = crate::palw_step_leg::step_opening_v1(&self.leaf_hashes(), u64::from(id)).ok()?;
        Some(PalwTokenTableOpeningV1 { id, rendering: entry.clone(), opening })
    }

    /// The opening of the end-of-generation list against [`Self::root`].
    pub fn eog_opening(&self) -> Option<PalwTokenTableEogOpeningV1> {
        let opening = crate::palw_step_leg::step_opening_v1(&self.leaf_hashes(), self.entries.len() as u64).ok()?;
        let mut ids = self.eog_token_ids.clone();
        ids.sort_unstable();
        Some(PalwTokenTableEogOpeningV1 { ids, opening })
    }

    /// **The table's digest** — what a job's `tokenizer_id` names for a class that serves constrained decoding: keyed
    /// BLAKE2b-512 over the vocabulary size, every entry (a presence byte, then length and bytes) and the sorted
    /// end-of-generation ids.
    pub fn digest(&self) -> Hash64 {
        let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_TOKEN_TABLE_DOMAIN_V1).to_state();
        state.update(&(self.entries.len() as u64).to_le_bytes());
        for entry in &self.entries {
            match entry {
                None => {
                    state.update(&[0]);
                }
                Some(bytes) => {
                    state.update(&[1]);
                    state.update(&(bytes.len() as u32).to_le_bytes());
                    state.update(bytes);
                }
            }
        }
        let mut eog = self.eog_token_ids.clone();
        eog.sort_unstable();
        state.update(&(eog.len() as u32).to_le_bytes());
        for id in eog {
            state.update(&id.to_le_bytes());
        }
        let mut out = [0u8; 64];
        out.copy_from_slice(state.finalize().as_bytes());
        Hash64::from_bytes(out)
    }
}

// ---------------------------------------------------------------------------------------------
// Distribution: a table file, and the process's held tables
// ---------------------------------------------------------------------------------------------

/// The magic of a token-table file: `PALWTTB1`, then the table's borsh.
pub const PALW_TOKEN_TABLE_FILE_MAGIC_V1: &[u8; 8] = b"PALWTTB1";
/// The most ids a table file may hold (a class's vocabulary is below 2^20; the court's tree is below 2^25).
pub const PALW_TOKEN_TABLE_MAX_VOCAB_V1: usize = 1 << 22;
/// The most bytes a table file may be (a 152k-entry table is a few MiB).
pub const PALW_TOKEN_TABLE_MAX_FILE_BYTES_V1: usize = 256 << 20;

/// **A table's file bytes** — what a worker emits (`--emit-token-table`) and a seat loads (`--palw-token-table`).
pub fn palw_token_table_file_encode_v1(table: &PalwTokenTableV1) -> Vec<u8> {
    let mut out = PALW_TOKEN_TABLE_FILE_MAGIC_V1.to_vec();
    out.extend(borsh::to_vec(table).expect("a table is borsh-serializable"));
    out
}

/// **A table from its file bytes**, refused by name when it is hostile (bad magic, trailing bytes, past the caps, no
/// end-of-generation id, an end-of-generation id outside the vocabulary). The loader derives the root itself
/// ([`PalwTokenTableV1::root`]); nothing in the file is trusted to name it.
pub fn palw_token_table_file_decode_v1(bytes: &[u8]) -> Result<PalwTokenTableV1, String> {
    if bytes.len() > PALW_TOKEN_TABLE_MAX_FILE_BYTES_V1 {
        return Err(format!("a token table file is at most {PALW_TOKEN_TABLE_MAX_FILE_BYTES_V1} bytes"));
    }
    let body = bytes.strip_prefix(&PALW_TOKEN_TABLE_FILE_MAGIC_V1[..]).ok_or("not a token table file (bad magic)")?;
    let table: PalwTokenTableV1 = borsh::from_slice(body).map_err(|e| format!("the token table does not decode: {e}"))?;
    if table.entries.is_empty() || table.entries.len() > PALW_TOKEN_TABLE_MAX_VOCAB_V1 {
        return Err(format!("a token table holds 1..={PALW_TOKEN_TABLE_MAX_VOCAB_V1} ids, this one {}", table.entries.len()));
    }
    if table.eog_token_ids.is_empty() || table.eog_token_ids.iter().any(|id| *id as usize >= table.entries.len()) {
        return Err("a token table names at least one end-of-generation id, each inside its vocabulary".to_string());
    }
    Ok(table)
}

/// **The tables this process holds, by root** — what a seat's replay of a constrained claim looks its table up in. Registered at
/// start-up from `--palw-token-table` files; a claim names its table's root, so a table is found by what the CLAIM committed to,
/// and a seat that holds none of the claim's abstains (`Unverifiable`).
#[derive(Default)]
pub struct PalwTokenTableSetV1 {
    by_root: std::sync::RwLock<std::collections::BTreeMap<Hash64, Arc<PalwTokenTableV1>>>,
}

impl PalwTokenTableSetV1 {
    pub fn register(&self, table: PalwTokenTableV1) -> Hash64 {
        let root = table.root();
        self.by_root.write().unwrap_or_else(|e| e.into_inner()).insert(root, Arc::new(table));
        root
    }

    pub fn for_root(&self, root: &Hash64) -> Option<Arc<PalwTokenTableV1>> {
        self.by_root.read().unwrap_or_else(|e| e.into_inner()).get(root).cloned()
    }

    pub fn len(&self) -> usize {
        self.by_root.read().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

static PALW_TOKEN_TABLES_V1: std::sync::OnceLock<PalwTokenTableSetV1> = std::sync::OnceLock::new();

/// The process-wide table set.
pub fn palw_token_tables_v1() -> &'static PalwTokenTableSetV1 {
    PALW_TOKEN_TABLES_V1.get_or_init(PalwTokenTableSetV1::default)
}

/// **The table a constrained job names, if this process holds it** (`None` for any other job).
pub fn palw_token_table_for_job_v1(job: &PalwFreePromptJobV3) -> Option<Arc<PalwTokenTableV1>> {
    palw_token_tables_v1().for_root(&palw_fp_constraint_table_root_v1(job)?)
}

/// Key of a token-table leaf.
pub const PALW_TOKEN_TABLE_DOMAIN_LEAF_V1: &[u8] = b"misaka-palw/token-table/leaf/v1";
/// Key of the end-of-generation leaf.
pub const PALW_TOKEN_TABLE_DOMAIN_EOG_V1: &[u8] = b"misaka-palw/token-table/eog/v1";

/// One id's leaf: `H(key, id ‖ presence ‖ le32(len) ‖ bytes)`.
pub fn table_leaf_hash_v1(id: u32, rendering: Option<&[u8]>) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_TOKEN_TABLE_DOMAIN_LEAF_V1).to_state();
    state.update(&id.to_le_bytes());
    match rendering {
        None => {
            state.update(&[0]);
        }
        Some(bytes) => {
            state.update(&[1]);
            state.update(&(bytes.len() as u32).to_le_bytes());
            state.update(bytes);
        }
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// The end-of-generation leaf: `H(key, le32(count) ‖ sorted ids)`.
pub fn table_eog_leaf_hash_v1(eog_token_ids: &[u32]) -> Hash64 {
    let mut ids = eog_token_ids.to_vec();
    ids.sort_unstable();
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_TOKEN_TABLE_DOMAIN_EOG_V1).to_state();
    state.update(&(ids.len() as u32).to_le_bytes());
    for id in ids {
        state.update(&id.to_le_bytes());
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **One id's rendering, opened against a table root** — what the court is handed instead of a 150,000-entry table.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTokenTableOpeningV1 {
    pub id: u32,
    pub rendering: Option<Vec<u8>>,
    pub opening: crate::palw_step_leg::PalwStepOpeningV1,
}

impl PalwTokenTableOpeningV1 {
    /// Does this opening hold id `self.id`'s rendering under `root`, in a table of `vocab` ids?
    pub fn verifies(&self, root: &Hash64, vocab: u32) -> bool {
        self.opening.leaf_index == u64::from(self.id)
            && self.id < vocab
            && self.opening.leaf_hash == table_leaf_hash_v1(self.id, self.rendering.as_deref())
            && crate::palw_step_leg::step_opening_root_v1(u64::from(vocab) + 1, &self.opening).is_ok_and(|r| r == *root)
    }
}

/// **The end-of-generation list, opened against a table root.**
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTokenTableEogOpeningV1 {
    pub ids: Vec<u32>,
    pub opening: crate::palw_step_leg::PalwStepOpeningV1,
}

impl PalwTokenTableEogOpeningV1 {
    pub fn verifies(&self, root: &Hash64, vocab: u32) -> bool {
        self.opening.leaf_index == u64::from(vocab)
            && self.ids.windows(2).all(|w| w[0] < w[1])
            && self.opening.leaf_hash == table_eog_leaf_hash_v1(&self.ids)
            && crate::palw_step_leg::step_opening_root_v1(u64::from(vocab) + 1, &self.opening).is_ok_and(|r| r == *root)
    }

    /// The lowest end-of-generation id the opened list names.
    pub fn lowest(&self) -> Option<u32> {
        self.ids.first().copied()
    }
}

/// **The court's third arm, as an accusation** (ADR-0096 Decision 7): what a challenger carries to convict a constrained
/// claim's committed token — the tiled decode pin (the claim's own row, opened), the claim's job (whose id the binding's
/// context pins, so the constraint bytes and the table root in its tail are the CLAIM's, never the challenger's), and the
/// renderings the rule reads, each opened against the job's table root: every id before the challenged position, the committed
/// token, the end-of-generation list, and — when the committed token is the lowest end-of-generation id — one lane the
/// constraint admits (the witness that the stop rule's "nothing is admitted" did not hold). Two-disclosure cases (the committed
/// token IS admitted and a better admitted lane exists) add the beating lane's rendering.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwConstrainedDecodeAccusationV1 {
    pub pin: crate::palw_step_refute::PalwTiledDecodePinV1,
    pub job: PalwFreePromptJobV3,
    /// Renderings opened against the job's table root: ids before the position, the committed id, the beating lane when the
    /// pin carries tiles, and the witness lane.
    pub renderings: Vec<PalwTokenTableOpeningV1>,
    pub eog: PalwTokenTableEogOpeningV1,
    /// A lane admitted from the state at the position (an id in `renderings`), named when it is needed.
    pub witness: Option<u32>,
}

// ---------------------------------------------------------------------------------------------
// The mask
// ---------------------------------------------------------------------------------------------

/// **A job's constraint and the class table it is read through** — everything one position's mask needs.
#[derive(Clone, Debug)]
pub struct PalwConstraintMaskV1 {
    pub constraint: PalwDecodeConstraintV1,
    pub table: Arc<PalwTokenTableV1>,
}

impl PartialEq for PalwConstraintMaskV1 {
    fn eq(&self, other: &Self) -> bool {
        self.constraint == other.constraint && Arc::ptr_eq(&self.table, &other.table)
    }
}
impl Eq for PalwConstraintMaskV1 {}

impl PalwConstraintMaskV1 {
    /// **The mask of `job` over `table`**, or why the host cannot judge this job: the job is a constrained job whose
    /// `tokenizer_id` is the table's digest, whose constraint bytes parse and whose table names an end-of-generation id.
    pub fn for_job(job: &PalwFreePromptJobV3, table: Arc<PalwTokenTableV1>) -> Result<Arc<Self>, String> {
        let Some(bytes) = palw_fp_constraint_tail_v1(job) else {
            return Err("the job is not a constrained job (version 6 with its constraint)".to_string());
        };
        if palw_fp_constraint_table_root_v1(job) != Some(table.root()) {
            return Err("this host's token table is not the table the job names (its root differs)".to_string());
        }
        if table.tokenizer_id != job.tokenizer_id {
            return Err("this host's token table was derived under another tokenizer than the job's tokenizer_id".to_string());
        }
        if table.lowest_eog_id().is_none() {
            return Err("the class table names no end-of-generation id, so the stop rule cannot be applied".to_string());
        }
        let constraint = palw_constraint_of_bytes_v1(bytes).map_err(|e| e.to_string())?;
        Ok(Arc::new(Self { constraint, table }))
    }

    /// The automaton state after the rendered prefix `ids`; `None` when an id has no rendering or the prefix is not
    /// admitted (a dead state).
    pub fn state_after(&self, ids: &[u32]) -> Option<PalwConstraintStateV1> {
        let mut renderings = Vec::with_capacity(ids.len());
        for id in ids {
            // An end-of-generation id renders nothing and ends the answer; it never appears inside a live prefix.
            renderings.push(self.table.rendering(*id)?);
        }
        constraint_state_after_v1(&self.constraint, renderings.iter())
    }

    /// `admit` for one lane from `state`: the next state, or `None` when the lane is not admitted.
    pub fn admit(&self, state: &PalwConstraintStateV1, lane: u32) -> Option<PalwConstraintStateV1> {
        constraint_admits_lane_v1(&self.constraint, state, self.table.rendering(lane))
    }

    /// The mask at one position: `admitted[j]` for every lane of a `lanes`-wide row.
    pub fn admitted_lanes(&self, state: &PalwConstraintStateV1, lanes: usize) -> Vec<bool> {
        (0..lanes as u32).map(|lane| self.admit(state, lane).is_some()).collect()
    }

    /// **Decision 7's selection at one position**: the argmax over the admitted lanes under the job's greedy key, or the
    /// lowest end-of-generation id when no lane is admitted (`true` in the second field: the run stops here). A dead
    /// `state` (a prefix the automaton does not admit) selects as if nothing were admitted.
    pub fn select(&self, state: Option<&PalwConstraintStateV1>, row: &[i32], position: u32, sampling: &crate::palw_decode_select_v2::PalwDecodeSamplingV2) -> (u32, bool) {
        let admitted = state.map(|s| self.admitted_lanes(s, row.len()));
        let lane = admitted.as_ref().and_then(|admitted| {
            crate::palw_decode_constraint_v1::decode_token_select_v3(row, &sampling.seed, position, sampling.temperature_q, |j| admitted[j])
        });
        match lane {
            Some(lane) => (lane as u32, false),
            None => (self.table.lowest_eog_id().unwrap_or(0), true),
        }
    }
}

thread_local! {
    static PALW_FP_CONSTRAINT_SCOPE: std::cell::RefCell<Option<Arc<PalwConstraintMaskV1>>> = const { std::cell::RefCell::new(None) };
}

/// **Run `f` with `mask` as this thread's constraint scope** — the scope a producer opens around one capture and a seat
/// around one verification of a constrained claim. Restored on exit, panics included.
pub fn palw_fp_with_constraint_scope_v1<R>(mask: Option<Arc<PalwConstraintMaskV1>>, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<Arc<PalwConstraintMaskV1>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0.take();
            PALW_FP_CONSTRAINT_SCOPE.with(|slot| *slot.borrow_mut() = previous);
        }
    }
    let previous = PALW_FP_CONSTRAINT_SCOPE.with(|slot| std::mem::replace(&mut *slot.borrow_mut(), mask));
    let _restore = Restore(previous);
    f()
}

/// This thread's constraint scope, if one is open.
pub fn palw_fp_constraint_scope_v1() -> Option<Arc<PalwConstraintMaskV1>> {
    PALW_FP_CONSTRAINT_SCOPE.with(|slot| slot.borrow().clone())
}

/// **The mask a HOST opens for `job` from the table it holds**: `Ok(None)` for any job that is not constrained; for a
/// constrained one `Ok(Some(mask))` when the host's table is the job's, and an error saying why not otherwise. What a seat's
/// replay and a producer's run call before they open [`palw_fp_with_constraint_scope_v1`].
pub fn palw_fp_constraint_mask_for_host_v1(
    job: &PalwFreePromptJobV3,
    table: Option<Arc<PalwTokenTableV1>>,
) -> Result<Option<Arc<PalwConstraintMaskV1>>, String> {
    if !job.is_constraint() {
        return Ok(None);
    }
    let table = table.ok_or_else(|| "this host holds no token table for the class".to_string())?;
    PalwConstraintMaskV1::for_job(job, table).map(Some)
}

/// **Does this host hold what a constrained job needs?** `Ok` for any other job; for a constrained one, a scope whose
/// constraint is the job's. The pre-run check of every producer's driver and every seat's replay.
pub fn palw_fp_constraint_scope_ready_v1(job: &PalwFreePromptJobV3) -> Result<(), String> {
    if !job.is_constraint() {
        return Ok(());
    }
    let bytes = palw_fp_constraint_tail_v1(job).ok_or_else(|| "a constrained job without its constraint".to_string())?;
    let scope = palw_fp_constraint_scope_v1().ok_or_else(|| {
        "this job carries a decode constraint (ADR-0096) and this host holds no token table for it: file Incapable, not a verdict".to_string()
    })?;
    let constraint = palw_constraint_of_bytes_v1(bytes).map_err(|e| e.to_string())?;
    if scope.constraint != constraint {
        return Err("the open constraint scope is another job's".to_string());
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The job's own rules and the door
// ---------------------------------------------------------------------------------------------

/// Why a job is not a well-formed constrained job.
#[derive(thiserror::Error, Clone, Debug, PartialEq, Eq)]
pub enum PalwFpConstraintErrorV1 {
    #[error("palw_fp_decode_constraint is not in force")]
    NotArmed,
    #[error("job version {version} is not the constrained job (version 6)")]
    NotAConstraintJob { version: u16 },
    #[error("a constrained job is a V3 job: it carries no decode rules")]
    HasDecode,
    #[error("a constrained job carries its constraint in its tail")]
    NoTail,
    #[error("the constraint is not admitted: {0}")]
    Constraint(String),
    #[error("the payload does not decode")]
    Undecodable,
    #[error("a V3 rule refused the job: {0}")]
    V3(PalwFpV3Error),
}

/// The constraint bytes a constrained job carries — `None` for any other job.
pub fn palw_fp_constraint_tail_v1(job: &PalwFreePromptJobV3) -> Option<&Vec<u8>> {
    match (&job.tail, job.is_constraint()) {
        (Some(PalwFpJobTailV1::Constraint(tail)), true) => Some(&tail.constraint),
        _ => None,
    }
}

/// A constraint's bytes parsed under whichever form their header names (both bounds; the height rule for the second form is
/// the header-context door's).
pub fn palw_constraint_of_bytes_v1(bytes: &[u8]) -> Result<PalwDecodeConstraintV1, crate::palw_fp_constraint_v2::PalwConstraintFormErrorV1> {
    crate::palw_fp_constraint_v2::palw_constraint_admitted_v1(bytes, true)
}

/// The table root a constrained job names — `None` for any other job.
pub fn palw_fp_constraint_table_root_v1(job: &PalwFreePromptJobV3) -> Option<Hash64> {
    match (&job.tail, job.is_constraint()) {
        (Some(PalwFpJobTailV1::Constraint(tail)), true) => Some(tail.table_root),
        _ => None,
    }
}

/// The constraint's id, `constraint_id_v1` over its bytes — what the derivation's `grammar_id` and the entrance report.
pub fn palw_fp_constraint_id_v1(job: &PalwFreePromptJobV3) -> Option<Hash64> {
    palw_fp_constraint_tail_v1(job).map(|bytes| constraint_id_v1(bytes))
}

/// **A constrained job's own shape rules**: version 6, no decode rules, the constraint in the tail and well formed under
/// its form. The V3 rules run on its V3 stand-in in the callers.
pub fn palw_fp_constraint_shape_v1(job: &PalwFreePromptJobV3) -> Result<PalwDecodeConstraintV1, PalwFpConstraintErrorV1> {
    if !job.is_constraint() {
        return Err(PalwFpConstraintErrorV1::NotAConstraintJob { version: job.version });
    }
    if job.decode.is_some() {
        return Err(PalwFpConstraintErrorV1::HasDecode);
    }
    let Some(bytes) = palw_fp_constraint_tail_v1(job) else { return Err(PalwFpConstraintErrorV1::NoTail) };
    palw_constraint_of_bytes_v1(bytes).map_err(|e| PalwFpConstraintErrorV1::Constraint(e.to_string()))
}

/// **The constrained claim's V3 stand-in**: the same payload at FP Job V3 (version 5, no tail), so every rule the lane
/// applies to a V3 commitment applies unchanged and only the version rule is this module's.
pub fn palw_fp_constraint_stand_in_v1(payload: &PalwFpCommitmentTxPayloadV3) -> PalwFpCommitmentTxPayloadV3 {
    let mut stand_in = payload.clone();
    stand_in.commitment.job.version = crate::palw_freeprompt_v3::PALW_FP_V3_VERSION;
    stand_in.commitment.job.tail = None;
    stand_in
}

/// **Is this FP payload a constrained claim's?** Its job's version word (bytes 2..4) is [`PALW_FP_CONSTRAINT_VERSION`].
pub fn palw_fp_payload_is_constraint_v1(payload: &[u8]) -> bool {
    payload.get(2..4) == Some(&PALW_FP_CONSTRAINT_VERSION.to_le_bytes()[..])
}

fn constraint_refused(e: PalwFpConstraintErrorV1) -> PalwFpV3Error {
    PalwFpV3Error::ConstraintClaim(e.to_string())
}

/// **The isolation door for a constrained claim**, height-free: the payload decodes with its tail, its job's own shape holds
/// ([`palw_fp_constraint_shape_v1`]) and its V3 stand-in passes the lane's shape rules (under the V3 rule, whatever the
/// decode-rules fence says: a constrained job is a V3 job).
pub fn validate_palw_fp_constraint_commitment_tx_v1(
    payload: &[u8],
    panel_da_admissible: bool,
    prompt_ids_form: PalwPromptIdsFormV1,
    work_leaves_cap: u64,
) -> Result<(), PalwFpV3Error> {
    let payload: PalwFpCommitmentTxPayloadV3 =
        borsh::from_slice(payload).map_err(|_| constraint_refused(PalwFpConstraintErrorV1::Undecodable))?;
    palw_fp_constraint_shape_v1(&payload.commitment.job).map_err(constraint_refused)?;
    palw_fp_constraint_stand_in_v1(&payload).validate_shape_under_ruleset_v4(
        panel_da_admissible,
        work_leaves_cap,
        None,
        prompt_ids_form,
        PalwFpDecodeRulesV1::Dormant,
    )
}

/// **The free-prompt door with constrained claims**: a version-6 payload goes to
/// [`validate_palw_fp_constraint_commitment_tx_v1`] where `constraint_door` (the ruleset carries
/// `palw_fp_decode_constraint`); every other payload to the prefix-state-aware door under it, which refuses version 6 by
/// name where this door is shut, as a build without the fence does.
#[allow(clippy::too_many_arguments)]
pub fn validate_palw_fp_commitment_tx_under_v9(
    payload: &[u8],
    panel_da_admissible: bool,
    prompt_ids_form: PalwPromptIdsFormV1,
    work_leaves_cap: u64,
    decode_rules: PalwFpDecodeRulesV1,
    improvement_door: bool,
    gen_door: bool,
    prefix_door: bool,
    inherit_door: bool,
    constraint_door: bool,
) -> Result<(), PalwFpV3Error> {
    if constraint_door && palw_fp_payload_is_constraint_v1(payload) {
        return validate_palw_fp_constraint_commitment_tx_v1(payload, panel_da_admissible, prompt_ids_form, work_leaves_cap);
    }
    crate::palw_fp_prefix_v1::validate_palw_fp_commitment_tx_under_v8(
        payload,
        panel_da_admissible,
        prompt_ids_form,
        work_leaves_cap,
        decode_rules,
        improvement_door,
        gen_door,
        prefix_door,
        inherit_door,
    )
}

/// **Why the containing block's height refuses a constrained claim** — the header-context half of the door.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwFpConstraintHeightRefusalV1 {
    /// Below `Params::palw_fp_decode_constraint`: no constrained claim exists yet.
    BelowConstraint,
    /// A version-2 constraint form below `Params::palw_fp_constraint_v2`.
    SecondFormBelowItsFence,
}

impl PalwFpConstraintHeightRefusalV1 {
    pub fn why(self) -> &'static str {
        match self {
            Self::BelowConstraint => "a constrained claim (FP job version 6) below Params::palw_fp_decode_constraint",
            Self::SecondFormBelowItsFence => "a version-2 decode constraint below Params::palw_fp_constraint_v2",
        }
    }
}

/// **The header-context half of the constrained door**: at the containing block's height, why a constrained claim is
/// refused. `None` for any other payload and for one the height admits.
pub fn palw_fp_constraint_refusal_at_v1(
    payload: &[u8],
    constraint_active: bool,
    constraint_v2_active: bool,
) -> Option<PalwFpConstraintHeightRefusalV1> {
    if !palw_fp_payload_is_constraint_v1(payload) {
        return None;
    }
    if !constraint_active {
        return Some(PalwFpConstraintHeightRefusalV1::BelowConstraint);
    }
    if !constraint_v2_active
        && let Ok(decoded) = borsh::from_slice::<PalwFpCommitmentTxPayloadV3>(payload)
        && let Some(bytes) = palw_fp_constraint_tail_v1(&decoded.commitment.job)
        && crate::palw_fp_constraint_v2::palw_constraint_admitted_v1(bytes, false).is_err()
        && crate::palw_fp_constraint_v2::palw_constraint_admitted_v1(bytes, true).is_ok()
    {
        return Some(PalwFpConstraintHeightRefusalV1::SecondFormBelowItsFence);
    }
    None
}

/// What the extraction walk reads of a payload when `constraint_armed`: for a constrained claim, its V3 stand-in; for any
/// other payload, itself. `Err` is the walk's skip reason. A second-form constraint is skipped below its fence.
pub fn palw_fp_constraint_walk_view_v1(
    payload: &PalwFpCommitmentTxPayloadV3,
    constraint_armed: bool,
    constraint_v2_armed: bool,
) -> Result<PalwFpCommitmentTxPayloadV3, &'static str> {
    if !payload.commitment.job.is_constraint() {
        return Ok(payload.clone());
    }
    if !constraint_armed {
        return Err("a constrained claim (FP job version 6) below palw_fp_decode_constraint");
    }
    let Some(bytes) = palw_fp_constraint_tail_v1(&payload.commitment.job) else {
        return Err("a constrained claim whose job is not well formed");
    };
    if crate::palw_fp_constraint_v2::palw_constraint_admitted_v1(bytes, constraint_v2_armed).is_err() {
        return Err("a constrained claim whose constraint is not admitted at this height");
    }
    if payload.commitment.job.decode.is_some() {
        return Err("a constrained claim whose job is not well formed");
    }
    Ok(palw_fp_constraint_stand_in_v1(payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::TX_VERSION;
    use crate::palw_decode_constraint_v1::{
        PalwConstraintActionV1, PalwConstraintEdgeV1, PalwConstraintFrameV1, PalwConstraintNodeV1, PALW_DECODE_CONSTRAINT_VERSION_V1,
    };
    use crate::palw_decode_pipeline_v4::{PalwFpDecodeStopReasonV1, PalwFpDecoderV1};
    use crate::palw_decode_select_v2::PalwDecodeSamplingV2;
    use crate::palw_fp_objects_v3::{PalwFpClassCapsV1, PalwFpDerivedWorkCapV1, palw_fp_objects_from_accepted_txs_by_class_v1};
    use crate::palw_freeprompt_v3::{
        PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_V3_VERSION, PalwFpStopReasonV3, PalwFreePromptCommitmentV3, fp_job_id_v3, fp_trace_manifest_v3,
    };
    use crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT;
    use crate::tx::{Transaction, TransactionId, TransactionOutpoint};

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    /// An automaton for exactly the byte strings `ab` and `ba`.
    fn ab_or_ba() -> PalwDecodeConstraintV1 {
        let node = |accepting, edges: Vec<(u8, u16)>| PalwConstraintNodeV1 {
            accepting,
            edges: edges.into_iter().map(|(b, to)| PalwConstraintEdgeV1 { lo: b, hi: b, action: PalwConstraintActionV1::Goto(to) }).collect(),
        };
        let c = PalwDecodeConstraintV1 {
            version: PALW_DECODE_CONSTRAINT_VERSION_V1,
            compiler_id: h64(0x96),
            start_frame: 0,
            frames: vec![PalwConstraintFrameV1 {
                start: 0,
                nodes: vec![
                    node(false, vec![(b'a', 1), (b'b', 2)]),
                    node(false, vec![(b'b', 3)]),
                    node(false, vec![(b'a', 3)]),
                    node(true, vec![]),
                ],
            }],
        };
        c.validate().expect("well formed");
        c
    }

    /// Ids 0..6 render `a`, `b`, `c`, `a`, `b`, and id 5 ends generation.
    fn table() -> Arc<PalwTokenTableV1> {
        let mut t = PalwTokenTableV1 {
            entries: [b'a', b'b', b'c', b'a', b'b', b'.'].iter().map(|b| Some(vec![*b])).collect(),
            eog_token_ids: vec![5],
            tokenizer_id: Hash64::default(),
        };
        t.tokenizer_id = h64(0x70);
        Arc::new(t)
    }

    fn job(constraint: Vec<u8>, limit: u32) -> PalwFreePromptJobV3 {
        PalwFreePromptJobV3 {
            version: PALW_FP_CONSTRAINT_VERSION,
            network_domain: h64(0x4E),
            class_id: h64(1),
            executor_bond: TransactionOutpoint { transaction_id: TransactionId::from_u64_word(7), index: 0 },
            executor_pubkey: vec![7; 32],
            operator_id: h64(0xE0),
            anchor_block: h64(0xA0),
            anchor_daa: 5_000,
            job_nonce: [0x11; 32],
            tokenizer_id: h64(0x70),
            prompt_token_ids_hash: crate::palw_v2::prompt_token_ids_hash_v2(&(0..12).collect::<Vec<u32>>()),
            prompt_tokens: 12,
            decode_token_limit: limit,
            max_context_tokens: 4_096,
            privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: crate::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER,
            sampling_seed: crate::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
            temperature_q: crate::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
            decode: None,
            tail: Some(PalwFpJobTailV1::Constraint(PalwFpConstraintTailV1 { constraint, table_root: table().root() })),
        }
    }

    fn payload(constraint: Vec<u8>) -> PalwFpCommitmentTxPayloadV3 {
        let ids: Vec<u32> = (0..12).collect();
        let decode = 3u32;
        let events: Vec<Hash64> = (0..decode as u64).map(|i| h64(i + 1)).collect();
        let (manifest_root, chunk_count, _) = fp_trace_manifest_v3(h64(0xB1), &events);
        PalwFpCommitmentTxPayloadV3 {
            version: PALW_FP_V3_VERSION,
            commitment: PalwFreePromptCommitmentV3 {
                trace_root: h64(0x7A),
                output_root: h64(0x0B),
                execution_root: h64(0x4E),
                schedule_root: h64(0x5C),
                decode_tokens_executed: decode,
                stop_reason: PalwFpStopReasonV3::EndOfGeneration,
                work_leaves: (12u64 + decode as u64) * 64,
                trace_manifest_root: manifest_root,
                trace_chunk_count: chunk_count,
                trace_retention_daa: 505_000,
                job: PalwFreePromptJobV3 { decode_token_limit: 8, ..job(constraint, 8) },
            },
            prompt_token_ids: ids,
            signature: vec![0x5A; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        }
    }

    fn freeprompt() -> crate::palw_freeprompt_v3::PalwFreePromptParamsV3 {
        crate::palw_fp_devnet_v3::palw_fp_devnet_bundle_for_tests(h64(1), h64(0xCA7), h64(0xC0757)).unwrap().freeprompt
    }

    #[test]
    fn the_job_wire_and_id_are_the_versions_own() {
        let bytes = ab_or_ba().to_bytes();
        let j = job(bytes.clone(), 8);
        assert!(j.is_constraint() && !j.decodes_under_v4_rules());
        assert_eq!(borsh::from_slice::<PalwFreePromptJobV3>(&borsh::to_vec(&j).unwrap()).unwrap(), j, "the wire round-trips");
        assert_eq!(palw_fp_constraint_tail_v1(&j), Some(&bytes));
        assert_eq!(palw_fp_constraint_id_v1(&j), Some(constraint_id_v1(&bytes)));
        let as_v3 = PalwFreePromptJobV3 { version: PALW_FP_V3_VERSION, tail: None, ..j.clone() };
        assert_ne!(fp_job_id_v3(&j), fp_job_id_v3(&as_v3), "its own domain");
        let mut other = ab_or_ba();
        other.compiler_id = h64(0x97);
        assert_ne!(fp_job_id_v3(&j), fp_job_id_v3(&job(other.to_bytes(), 8)), "the constraint is inside the id");
        assert!(palw_fp_payload_is_constraint_v1(&borsh::to_vec(&payload(bytes)).unwrap()));
        assert!(!palw_fp_payload_is_constraint_v1(&borsh::to_vec(&palw_fp_constraint_stand_in_v1(&payload(ab_or_ba().to_bytes()))).unwrap()));
    }

    #[test]
    fn the_token_table_is_a_digest_and_a_mask_needs_the_jobs_tokenizer() {
        let t = table();
        let same = PalwTokenTableV1 { tokenizer_id: h64(0x71), ..(*t).clone() };
        assert_eq!(t.digest(), same.digest(), "the digest does not cover the tokenizer id");
        let other = PalwTokenTableV1 { eog_token_ids: vec![4], ..(*t).clone() };
        assert_ne!(t.digest(), other.digest());
        assert_eq!((t.rendering(0), t.rendering(5), t.rendering(99)), (Some(&b"a"[..]), None, None), "an eog id and an unknown id render nothing");
        let j = job(ab_or_ba().to_bytes(), 8);
        assert!(PalwConstraintMaskV1::for_job(&j, t.clone()).is_ok());
        assert!(PalwConstraintMaskV1::for_job(&j, Arc::new(same)).unwrap_err().contains("another tokenizer"));
        let no_eog = PalwTokenTableV1 { eog_token_ids: vec![], ..(*t).clone() };
        assert!(PalwConstraintMaskV1::for_job(&j, Arc::new(no_eog)).unwrap_err().contains("end-of-generation"));
        assert!(palw_fp_constraint_mask_for_host_v1(&j, None).is_err(), "no table, no mask");
        assert_eq!(palw_fp_constraint_mask_for_host_v1(&PalwFreePromptJobV3 { version: PALW_FP_V3_VERSION, tail: None, ..j }, None).unwrap(), None);
    }

    #[test]
    fn the_decoder_commits_the_admitted_argmax_and_the_lowest_eog_when_nothing_is_admitted() {
        let j = job(ab_or_ba().to_bytes(), 8);
        let mask = PalwConstraintMaskV1::for_job(&j, table()).unwrap();
        // Every row prefers id 2 (`c`, forbidden) then id 4 (`b`), then 0 (`a`).
        let row = [10, 0, 100, 0, 50, 99];
        let mut decoder = PalwFpDecoderV1::v3(PalwDecodeSamplingV2::GREEDY, 8).with_constraint(Some(mask.clone()));
        let fed: Vec<u32> = (0..4).map(|_| decoder.select(&row)).collect();
        // `b` (id 4, the best admitted), then `a` (the only admitted from there: ids 0 and 3, the lowest wins the tie), then
        // the string is complete: nothing is admitted and the lowest end-of-generation id commits and ends the run.
        assert_eq!(fed[..3], [4, 0, 5]);
        let stop = decoder.stop().expect("the run stopped");
        assert_eq!((stop.executed, stop.reason), (3, PalwFpDecodeStopReasonV1::NoAdmissibleLane));
        assert_eq!(decoder.generated(), &[4, 0, 5][..], "the end-of-generation id is the answer's last committed id");
        // Without the scope the same job is the plain V3 rule: the mask is the host's to open.
        let mut plain = j.decoder_v1();
        assert_eq!(plain.select(&row), 2, "no scope: the plain V3 argmax (the forbidden id 2)");
        // With the scope open, `decoder_v1` is the masked decoder.
        let masked = palw_fp_with_constraint_scope_v1(Some(mask), || {
            let mut d = j.decoder_v1();
            (0..3).map(|_| d.select(&row)).collect::<Vec<u32>>()
        });
        assert_eq!(masked, [4, 0, 5]);
        assert!(palw_fp_constraint_scope_v1().is_none(), "the scope is restored");
        assert!(palw_fp_constraint_scope_ready_v1(&j).is_err());
    }

    #[test]
    fn the_replay_rule_selects_what_the_producer_selected() {
        let j = job(ab_or_ba().to_bytes(), 8);
        let mask = PalwConstraintMaskV1::for_job(&j, table()).unwrap();
        let row = [10, 0, 100, 0, 50, 99];
        let committed = [4u32, 0, 5];
        let ids: Vec<u32> = palw_fp_with_constraint_scope_v1(Some(mask), || {
            let rule = crate::palw_decode_pipeline_v4::PalwFpReplayRuleV1::of_job(&j, &committed).expect("a constrained rule");
            (0..3).map(|r| rule.select(&row, r)).collect()
        });
        assert_eq!(ids, committed);
        assert!(crate::palw_decode_pipeline_v4::PalwFpReplayRuleV1::of_job(&j, &committed).is_none(), "a host with no scope has no rule");
    }

    fn tx(bytes: Vec<u8>) -> Transaction {
        Transaction::new(TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_PALW_FP_COMMITMENT, 0, bytes)
    }

    #[test]
    fn the_doors_admit_the_constrained_claim_only_where_and_when_the_fence_says() {
        let bytes = ab_or_ba().to_bytes();
        let wire = borsh::to_vec(&payload(bytes.clone())).unwrap();
        let door = |constraint_door: bool, rules: PalwFpDecodeRulesV1| {
            validate_palw_fp_commitment_tx_under_v9(&wire, false, PalwPromptIdsFormV1::Flat, 1 << 26, rules, false, false, false, false, constraint_door)
        };
        assert!(door(true, PalwFpDecodeRulesV1::Dormant).is_ok(), "the door is open where the ruleset carries the fence");
        assert!(door(true, PalwFpDecodeRulesV1::Active).is_ok(), "a constrained job is a V3 job whatever the decode rules say");
        assert!(matches!(door(false, PalwFpDecodeRulesV1::Dormant), Err(PalwFpV3Error::UnsupportedVersion { got: 6, .. })), "shut: refused by name");
        // Its own shape.
        // (A version-6 payload carrying decode rules is not a wire form at all: its job decodes no decode tail.)
        let junk = payload(vec![9, 9, 9]);
        assert!(validate_palw_fp_constraint_commitment_tx_v1(&borsh::to_vec(&junk).unwrap(), false, PalwPromptIdsFormV1::Flat, 1 << 26).is_err());
        // The height door.
        assert_eq!(palw_fp_constraint_refusal_at_v1(&wire, false, false), Some(PalwFpConstraintHeightRefusalV1::BelowConstraint));
        assert_eq!(palw_fp_constraint_refusal_at_v1(&wire, true, false), None);
        assert_eq!(palw_fp_constraint_refusal_at_v1(&[0, 0, 7, 0], false, false), None, "another version is not this door's");
    }

    #[test]
    fn the_walk_carries_a_constrained_claim_on_its_stand_in_and_skips_it_by_name_otherwise() {
        let bytes = ab_or_ba().to_bytes();
        let p = payload(bytes);
        let walk = |armed: bool, v2: bool, rules: PalwFpDecodeRulesV1| {
            palw_fp_objects_from_accepted_txs_by_class_v1(
                &[tx(borsh::to_vec(&p).unwrap())],
                h64(0x4E),
                &freeprompt(),
                crate::BlockHash::default(),
                false,
                |_| PalwFpClassCapsV1 {
                    step_ladder: 1 << 26,
                    held: false,
                    derived_work: PalwFpDerivedWorkCapV1::Declared,
                    logits_q24: true,
                    prefix_state_armed: false,
                    prefix_inherit_armed: false, prefix_inherit_class_safe: false,
                    constraint_armed: armed,
                    constraint_v2_armed: v2,
                    tokenizer: crate::palw_fp_tokenizer_v1::PalwFpTokenizerRuleV1::Dormant,
                },
                false,
                false,
                PalwPromptIdsFormV1::Flat,
                rules,
                |_, _, _, _| true,
            )
        };
        let ok = walk(true, false, PalwFpDecodeRulesV1::Dormant);
        assert_eq!((ok.objects.len(), ok.skipped.len()), (1, 0), "{:?}", ok.skipped);
        // Past the decode rules a constrained job is still the V3 job it is.
        assert_eq!(walk(true, false, PalwFpDecodeRulesV1::Active).objects.len(), 1);
        let off = walk(false, false, PalwFpDecodeRulesV1::Dormant);
        assert_eq!(off.objects.len(), 0);
        assert!(off.skipped[0].1.contains("below palw_fp_decode_constraint"), "{:?}", off.skipped);
    }
}
