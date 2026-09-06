//! **ADR-0088 — the class keeps its graph; a line keeps its owner, and the owner keeps
//! publishing.**
//!
//! The pure half of the model registry, kept apart from the fold that applies it (the way
//! `palw_model_market_v1` is kept apart from ADR-0087's arms): the constants, the rows a line
//! and a version are, the ids a line and a proposal are keyed by, the messages the ten objects
//! are signed over, and the split of ADR-0087's registrant leg between a line's owner and an
//! adopted contributor. Everything here is a function of its arguments; the fold, the RPC, the
//! CLI and the tests read one arithmetic.
//!
//! A **class** is its graph — the unit of work, share, certification and the court — and none
//! of that moves when weights do. A **line** is a model in the market's sense: a class, an
//! owner bond, a name. The founding line of a class is its registrant's and has the class id as
//! its line id, so a chain where nothing happened has no line rows and ADR-0087's per-class
//! market is, byte for byte, the founding line's. A **version** is one signed object from the
//! line's developer: a root (weights), a parent, declared hashes the chain records and never
//! reads, and usage the fold counts.
//!
//! Every constant is a code constant and not a bundle field, on purpose: a `PalwStateParamsV2`
//! field moves `palw_ruleset_id_v2` and re-mints every network, while ADR-0088 Decision 11 says
//! the fingerprint moves only where `Params::palw_model_lines` is set.

use crate::Hash64;
use crate::palw_state_v2::PalwBondKeyV2;

// ---- the operator's numbers (ADR-0088 §4's examples, until the flag is armed) ---------------

/// Lines a class may hold (Decision 1).
pub const PALW_MODEL_LINES_PER_CLASS_V1: usize = 64;
/// Previews a line may hold at once (Decision 2).
pub const PALW_MODEL_PREVIEWS_V1: usize = 2;
/// Versions kept in state per line; older rows are evicted and live in the explorer (Decision 10).
pub const PALW_MODEL_VERSION_HISTORY_V1: u32 = 64;
/// Open proposals a line may hold (Decision 7).
pub const PALW_MODEL_PROPOSALS_PER_LINE_V1: usize = 32;
/// Evaluations a version may hold (Decision 5).
pub const PALW_MODEL_EVALUATIONS_PER_VERSION_V1: usize = 16;
/// How long a superseded version's root stays in force after a promotion (Decision 2).
pub const PALW_VERSION_GRACE_DAA_V1: u64 = 4_000;
/// A line's name, at most (Decision 1).
pub const PALW_MODEL_LINE_NAME_MAX_BYTES: usize = 64;
/// The rent a founding, a proposal and an evaluation burn (Decision 11; §4: 1 MSK).
pub const PALW_MODEL_OBJECT_RENT_SOMPI_V1: u64 = 100_000_000;

// ---- ids --------------------------------------------------------------------------------------

fn keyed64(domain: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    for part in parts {
        state.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// Decision 1: a non-founding line's id — the owner is in it, so a name is never squatted, only
/// shared. The founding line's id is the class id and is not derived here.
pub fn model_line_id_v1(class_id: &Hash64, founder: &PalwBondKeyV2, name: &[u8]) -> Hash64 {
    let bond = borsh::to_vec(founder).expect("a bond key is borsh-serializable");
    keyed64(b"misaka-palw/model-line/id/v1", &[class_id.as_byte_slice(), &bond, &(name.len() as u32).to_le_bytes(), name])
}

/// Decision 7: a proposal's id.
pub fn model_proposal_id_v1(line_id: &Hash64, root: &Hash64, by: &PalwBondKeyV2) -> Hash64 {
    let bond = borsh::to_vec(by).expect("a bond key is borsh-serializable");
    keyed64(b"misaka-palw/model-proposal/id/v1", &[line_id.as_byte_slice(), root.as_byte_slice(), &bond])
}

// ---- rows -------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwModelLineStatusV1 {
    Active = 0,
    /// Retired by its owner (Decision 6): the market is closed to buys, the roots leave force
    /// after the grace, the history stays.
    Retired = 1,
}

/// A line (Decision 1). A founding line has no row until something about it changes; the fold
/// synthesises its row from the class (`founding_line_v1`).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwModelLineV1 {
    pub class_id: Hash64,
    /// `None` only for a genesis class's founding line — unowned, one version, nobody publishes.
    pub owner: Option<PalwBondKeyV2>,
    /// The bond that publishes versions; `None` means the owner (Decision 6).
    pub developer: Option<PalwBondKeyV2>,
    /// The bond that may post the line's own evaluations; `None` means the owner (Decision 6).
    pub maintainer: Option<PalwBondKeyV2>,
    pub name: Vec<u8>,
    pub founded_daa: u64,
    /// The current version's number (1 at founding).
    pub current: u32,
    /// Versions published as previews and not yet promoted or withdrawn (Decision 2).
    pub previews: Vec<u32>,
    /// The commit count: versions are dense and monotone (Decision 2).
    pub versions_published: u32,
    /// The share of the registrant leg an adopted contributor takes while its version is
    /// current, in permille of the leg (Decisions 7 and 8).
    pub contributor_permille_of_leg: u16,
    pub status: PalwModelLineStatusV1,
    /// Retired lines keep the grace end here so the roots-in-force reader has one number.
    pub retired_daa: Option<u64>,
}

impl PalwModelLineV1 {
    /// The bond that may publish versions.
    pub fn developer_bond(&self) -> Option<PalwBondKeyV2> {
        self.developer.or(self.owner)
    }
    /// The bond that may post the line's own evaluations (the developer may too).
    pub fn maintainer_bond(&self) -> Option<PalwBondKeyV2> {
        self.maintainer.or(self.owner)
    }
    pub fn is_active(&self) -> bool {
        matches!(self.status, PalwModelLineStatusV1::Active)
    }
}

/// Decision 1: the founding line of a class, synthesised from the class row when no line row
/// exists yet. `owner` is the class's registrant bond; `name` the catalog name the registrant
/// wrote (empty when the chain holds none — the explorer fills it from the catalog).
pub fn founding_line_v1(class_id: Hash64, registrant: Option<PalwBondKeyV2>, name: Vec<u8>, founded_daa: u64) -> PalwModelLineV1 {
    PalwModelLineV1 {
        class_id,
        owner: registrant,
        developer: None,
        maintainer: None,
        name,
        founded_daa,
        current: 1,
        previews: Vec::new(),
        versions_published: 1,
        contributor_permille_of_leg: 0,
        status: PalwModelLineStatusV1::Active,
        retired_daa: None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwVersionStatusV1 {
    Current = 0,
    /// In force, not the default (Decision 2).
    Preview = 1,
    /// Was current; its root stays in force until `until_daa` (Decision 2).
    Superseded {
        until_daa: u64,
    } = 2,
    /// Taken out of force by the developer (Decision 2).
    Withdrawn = 3,
}

/// Decision 4: what the fold counted about a version — paid inferences, never quality.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwVersionUsageV1 {
    pub attempt_claims: u64,
    pub fp_claims: u64,
    pub work_leaves: u128,
    pub first_used_daa: Option<u64>,
    pub last_used_daa: Option<u64>,
}

impl PalwVersionUsageV1 {
    pub fn count(&mut self, attempt: bool, leaves: u64, daa: u64) {
        if attempt {
            self.attempt_claims = self.attempt_claims.saturating_add(1);
        } else {
            self.fp_claims = self.fp_claims.saturating_add(1);
        }
        self.work_leaves = self.work_leaves.saturating_add(leaves as u128);
        if self.first_used_daa.is_none() {
            self.first_used_daa = Some(daa);
        }
        self.last_used_daa = Some(self.last_used_daa.map_or(daa, |d| d.max(daa)));
    }
    /// A voided claim is subtracted at the voiding (Decision 4).
    pub fn uncount(&mut self, attempt: bool, leaves: u64) {
        if attempt {
            self.attempt_claims = self.attempt_claims.saturating_sub(1);
        } else {
            self.fp_claims = self.fp_claims.saturating_sub(1);
        }
        self.work_leaves = self.work_leaves.saturating_sub(leaves as u128);
    }
}

/// A version (Decision 2): the root, the lineage, the declarations, the usage.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwModelVersionV1 {
    pub root: Hash64,
    pub parent: Option<u32>,
    /// The proposal this version adopted (Decision 7), paid by Decision 8 while current.
    pub adopted_from: Option<Hash64>,
    /// **Declarations.** Recorded, labelled, never read by a rule (Decision 2).
    pub runtime_hash: Option<Hash64>,
    pub dataset_commitment: Option<Hash64>,
    pub training_config_hash: Option<Hash64>,
    pub notes_hash: Option<Hash64>,
    pub published_daa: u64,
    pub published_by: Option<PalwBondKeyV2>,
    pub status: PalwVersionStatusV1,
    pub usage: PalwVersionUsageV1,
}

impl PalwModelVersionV1 {
    /// Decision 3: is this version's root in force at `daa`?
    pub fn in_force_at(&self, daa: u64) -> bool {
        match self.status {
            PalwVersionStatusV1::Current | PalwVersionStatusV1::Preview => true,
            PalwVersionStatusV1::Superseded { until_daa } => daa < until_daa,
            PalwVersionStatusV1::Withdrawn => false,
        }
    }
}

/// A proposal (Decision 7): a root and a note from any bond, adopted or not.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwModelProposalV1 {
    pub line_id: Hash64,
    pub root: Hash64,
    pub note_hash: Hash64,
    pub by: PalwBondKeyV2,
    pub posted_daa: u64,
    /// The version that adopted it, once one did.
    pub adopted_in: Option<u32>,
}

/// An evaluation (Decision 5): a declaration, from anyone, saying who declared it.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwModelEvaluationV1 {
    pub evaluator_id: Hash64,
    pub score_permille: u32,
    pub report_hash: Hash64,
    pub posted_daa: u64,
    /// Posted by the line's developer or maintainer — the line's own word.
    pub is_lines_own: bool,
}

// ---- the leg (Decision 8) ---------------------------------------------------------------------

/// Split ADR-0087's registrant leg between the owner and an adopted contributor:
/// `(contributor, owner)`; the whole leg is the owner's when no contributor share applies.
pub fn split_owner_leg_v1(leg: u64, contributor_permille: u16, has_contributor: bool) -> (u64, u64) {
    if !has_contributor || contributor_permille == 0 {
        return (0, leg);
    }
    let permille = contributor_permille.min(1000) as u128;
    let contributor = ((leg as u128 * permille) / 1000) as u64;
    (contributor, leg - contributor)
}

// ---- signed messages ---------------------------------------------------------------------------

pub const PALW_MODEL_LINE_MLDSA87_CONTEXT: &[u8] = b"misaka-palw-model-line-v1";
pub const PALW_MODEL_VERSION_MLDSA87_CONTEXT: &[u8] = b"misaka-palw-model-version-v1";
pub const PALW_MODEL_PROPOSAL_MLDSA87_CONTEXT: &[u8] = b"misaka-palw-model-proposal-v1";
pub const PALW_MODEL_EVALUATION_MLDSA87_CONTEXT: &[u8] = b"misaka-palw-model-evaluation-v1";

fn opt(state: &mut blake2b_simd::State, h: Option<&Hash64>) {
    match h {
        Some(h) => {
            state.update(&[1u8]);
            state.update(h.as_byte_slice());
        }
        None => {
            state.update(&[0u8]);
        }
    }
}

fn opt_bond(state: &mut blake2b_simd::State, b: Option<&PalwBondKeyV2>) {
    match b {
        Some(b) => {
            state.update(&[1u8]);
            state.update(&borsh::to_vec(b).expect("a bond key is borsh-serializable"));
        }
        None => {
            state.update(&[0u8]);
        }
    }
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

fn begin(domain: &[u8], network_domain: Hash64, line_id: &Hash64) -> blake2b_simd::State {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    state.update(network_domain.as_byte_slice());
    state.update(line_id.as_byte_slice());
    state
}

/// `ModelLineFounded` (Decision 1), signed by the founder.
pub fn palw_model_line_founded_message_v1(
    network_domain: Hash64,
    class_id: &Hash64,
    name: &[u8],
    founder: &PalwBondKeyV2,
    root: &Hash64,
) -> Hash64 {
    let mut s = begin(b"misaka-palw/model-line/founded/v1", network_domain, class_id);
    s.update(&(name.len() as u32).to_le_bytes());
    s.update(name);
    s.update(&borsh::to_vec(founder).expect("a bond key is borsh-serializable"));
    s.update(root.as_byte_slice());
    finish(s)
}

/// `ModelVersionPublished` (Decision 2), signed by the developer.
#[allow(clippy::too_many_arguments)]
pub fn palw_model_version_message_v1(
    network_domain: Hash64,
    line_id: &Hash64,
    version: u32,
    root: &Hash64,
    parent: Option<u32>,
    adopted_from: Option<&Hash64>,
    runtime_hash: Option<&Hash64>,
    dataset_commitment: Option<&Hash64>,
    training_config_hash: Option<&Hash64>,
    notes_hash: Option<&Hash64>,
    preview: bool,
) -> Hash64 {
    let mut s = begin(b"misaka-palw/model-version/published/v1", network_domain, line_id);
    s.update(&version.to_le_bytes());
    s.update(root.as_byte_slice());
    match parent {
        Some(p) => {
            s.update(&[1u8]);
            s.update(&p.to_le_bytes());
        }
        None => {
            s.update(&[0u8]);
        }
    }
    opt(&mut s, adopted_from);
    opt(&mut s, runtime_hash);
    opt(&mut s, dataset_commitment);
    opt(&mut s, training_config_hash);
    opt(&mut s, notes_hash);
    s.update(&[preview as u8]);
    finish(s)
}

/// `ModelVersionPromoted` / `ModelVersionWithdrawn` (Decision 2), signed by the developer;
/// `kind` is `b"promote"` or `b"withdraw"`.
pub fn palw_model_version_move_message_v1(network_domain: Hash64, line_id: &Hash64, version: u32, kind: &[u8]) -> Hash64 {
    let mut s = begin(b"misaka-palw/model-version/move/v1", network_domain, line_id);
    s.update(&version.to_le_bytes());
    s.update(&(kind.len() as u32).to_le_bytes());
    s.update(kind);
    finish(s)
}

/// `ModelLineRolesSet` (Decision 6), signed by the owner.
pub fn palw_model_roles_message_v1(
    network_domain: Hash64,
    line_id: &Hash64,
    developer: Option<&PalwBondKeyV2>,
    maintainer: Option<&PalwBondKeyV2>,
    contributor_permille_of_leg: u16,
) -> Hash64 {
    let mut s = begin(b"misaka-palw/model-line/roles/v1", network_domain, line_id);
    opt_bond(&mut s, developer);
    opt_bond(&mut s, maintainer);
    s.update(&contributor_permille_of_leg.to_le_bytes());
    finish(s)
}

/// `ModelLineOwnerTransferred` (Decision 6), signed by the owner.
pub fn palw_model_transfer_message_v1(network_domain: Hash64, line_id: &Hash64, new_owner: &PalwBondKeyV2) -> Hash64 {
    let mut s = begin(b"misaka-palw/model-line/transfer/v1", network_domain, line_id);
    s.update(&borsh::to_vec(new_owner).expect("a bond key is borsh-serializable"));
    finish(s)
}

/// `ModelLineRetired` (Decision 6), signed by the owner.
pub fn palw_model_retire_message_v1(network_domain: Hash64, line_id: &Hash64) -> Hash64 {
    finish(begin(b"misaka-palw/model-line/retire/v1", network_domain, line_id))
}

/// `ModelProposalPosted` (Decision 7), signed by the proposer.
pub fn palw_model_proposal_message_v1(
    network_domain: Hash64,
    line_id: &Hash64,
    root: &Hash64,
    note_hash: &Hash64,
    by: &PalwBondKeyV2,
) -> Hash64 {
    let mut s = begin(b"misaka-palw/model-proposal/posted/v1", network_domain, line_id);
    s.update(root.as_byte_slice());
    s.update(note_hash.as_byte_slice());
    s.update(&borsh::to_vec(by).expect("a bond key is borsh-serializable"));
    finish(s)
}

/// `ModelProposalClosed` (Decision 7), signed by the developer.
pub fn palw_model_proposal_close_message_v1(network_domain: Hash64, line_id: &Hash64, proposal_id: &Hash64) -> Hash64 {
    let mut s = begin(b"misaka-palw/model-proposal/closed/v1", network_domain, line_id);
    s.update(proposal_id.as_byte_slice());
    finish(s)
}

/// `ModelEvaluationPosted` (Decision 5), signed by `by`.
pub fn palw_model_evaluation_message_v1(
    network_domain: Hash64,
    line_id: &Hash64,
    version: u32,
    evaluator_id: &Hash64,
    score_permille: u32,
    report_hash: &Hash64,
    by: &PalwBondKeyV2,
) -> Hash64 {
    let mut s = begin(b"misaka-palw/model-evaluation/posted/v1", network_domain, line_id);
    s.update(&version.to_le_bytes());
    s.update(evaluator_id.as_byte_slice());
    s.update(&score_permille.to_le_bytes());
    s.update(report_hash.as_byte_slice());
    s.update(&borsh::to_vec(by).expect("a bond key is borsh-serializable"));
    finish(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bond(i: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(i), 0))
    }

    #[test]
    fn a_line_id_carries_its_founder_so_a_name_is_shared_and_never_squatted() {
        let class = Hash64::from_u64_word(1);
        let a = model_line_id_v1(&class, &bond(1), b"QWEN-27B-001");
        let b = model_line_id_v1(&class, &bond(2), b"QWEN-27B-001");
        assert_ne!(a, b, "two founders, one name, two lines");
        assert_ne!(a, model_line_id_v1(&class, &bond(1), b"QWEN-27B-002"));
        assert_ne!(a, model_line_id_v1(&Hash64::from_u64_word(2), &bond(1), b"QWEN-27B-001"));
        assert_ne!(a, class, "a founded line never collides with the founding line's id");
    }

    #[test]
    fn the_founding_line_is_the_registrants_and_the_roles_default_to_the_owner() {
        let line = founding_line_v1(Hash64::from_u64_word(1), Some(bond(7)), b"qwen".to_vec(), 10);
        assert_eq!(line.developer_bond(), Some(bond(7)));
        assert_eq!(line.maintainer_bond(), Some(bond(7)));
        assert_eq!((line.current, line.versions_published), (1, 1));
        let genesis = founding_line_v1(Hash64::from_u64_word(1), None, Vec::new(), 0);
        assert_eq!(genesis.developer_bond(), None, "an unowned line has nobody to publish");
    }

    #[test]
    fn a_version_is_in_force_by_its_status_and_the_grace() {
        let mut v = PalwModelVersionV1 {
            root: Hash64::from_u64_word(9),
            parent: None,
            adopted_from: None,
            runtime_hash: None,
            dataset_commitment: None,
            training_config_hash: None,
            notes_hash: None,
            published_daa: 100,
            published_by: Some(bond(1)),
            status: PalwVersionStatusV1::Current,
            usage: PalwVersionUsageV1::default(),
        };
        assert!(v.in_force_at(100));
        v.status = PalwVersionStatusV1::Preview;
        assert!(v.in_force_at(100));
        v.status = PalwVersionStatusV1::Superseded { until_daa: 200 };
        assert!(v.in_force_at(199) && !v.in_force_at(200));
        v.status = PalwVersionStatusV1::Withdrawn;
        assert!(!v.in_force_at(0));
    }

    #[test]
    fn usage_counts_paid_work_and_a_voiding_takes_it_back() {
        let mut u = PalwVersionUsageV1::default();
        u.count(true, 7_708, 50);
        u.count(false, 1_000, 40);
        assert_eq!((u.attempt_claims, u.fp_claims, u.work_leaves), (1, 1, 8_708));
        assert_eq!((u.first_used_daa, u.last_used_daa), (Some(50), Some(50)));
        u.uncount(true, 7_708);
        assert_eq!((u.attempt_claims, u.work_leaves), (0, 1_000));
        u.uncount(true, 1);
        assert_eq!(u.attempt_claims, 0, "never below zero");
    }

    #[test]
    fn the_leg_is_the_owners_unless_the_owner_shares_it() {
        assert_eq!(split_owner_leg_v1(1_000, 0, true), (0, 1_000));
        assert_eq!(split_owner_leg_v1(1_000, 250, false), (0, 1_000), "no adopted contributor, no share");
        assert_eq!(split_owner_leg_v1(1_000, 250, true), (250, 750));
        assert_eq!(split_owner_leg_v1(1_001, 500, true), (500, 501), "the remainder is the owner's");
        assert_eq!(split_owner_leg_v1(1_000, 1_500, true), (1_000, 0), "capped at the whole leg");
    }

    #[test]
    fn every_message_binds_every_field() {
        let n = Hash64::from_u64_word(1);
        let (l, r, h) = (Hash64::from_u64_word(2), Hash64::from_u64_word(3), Hash64::from_u64_word(4));
        let m = palw_model_version_message_v1(n, &l, 2, &r, Some(1), None, None, None, None, None, false);
        assert_ne!(m, palw_model_version_message_v1(n, &l, 3, &r, Some(1), None, None, None, None, None, false));
        assert_ne!(m, palw_model_version_message_v1(n, &l, 2, &r, Some(1), None, None, None, None, None, true));
        assert_ne!(m, palw_model_version_message_v1(n, &l, 2, &r, Some(1), Some(&h), None, None, None, None, false));
        assert_ne!(m, palw_model_version_message_v1(n, &l, 2, &r, Some(1), None, Some(&h), None, None, None, false));
        assert_ne!(m, palw_model_version_message_v1(n, &l, 2, &r, None, None, None, None, None, None, false));
        assert_ne!(
            palw_model_version_move_message_v1(n, &l, 2, b"promote"),
            palw_model_version_move_message_v1(n, &l, 2, b"withdraw")
        );
        assert_ne!(palw_model_roles_message_v1(n, &l, None, None, 0), palw_model_roles_message_v1(n, &l, Some(&bond(1)), None, 0));
        assert_ne!(palw_model_roles_message_v1(n, &l, None, None, 0), palw_model_roles_message_v1(n, &l, None, None, 1));
        assert_ne!(palw_model_transfer_message_v1(n, &l, &bond(1)), palw_model_transfer_message_v1(n, &l, &bond(2)));
        assert_ne!(palw_model_retire_message_v1(n, &l), palw_model_retire_message_v1(n, &r));
        assert_ne!(palw_model_proposal_message_v1(n, &l, &r, &h, &bond(1)), palw_model_proposal_message_v1(n, &l, &r, &h, &bond(2)));
        assert_ne!(
            palw_model_evaluation_message_v1(n, &l, 1, &h, 700, &r, &bond(1)),
            palw_model_evaluation_message_v1(n, &l, 1, &h, 701, &r, &bond(1))
        );
        assert_ne!(
            palw_model_line_founded_message_v1(n, &l, b"a", &bond(1), &r),
            palw_model_line_founded_message_v1(n, &l, b"b", &bond(1), &r)
        );
        assert_ne!(model_proposal_id_v1(&l, &r, &bond(1)), model_proposal_id_v1(&l, &r, &bond(2)));
    }
}
