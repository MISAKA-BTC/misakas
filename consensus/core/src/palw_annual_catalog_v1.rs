//! **RFC-0019 — the annual model catalog: `ApprovedModels(h)` as an add-only chain of signed-off catalogs** (dormant; it is the value
//! of ADR-0179's fence `palw_model_tier_v1`, which is `None` on every preset and refused when armed). Nothing here is built or run by
//! the lane that wrote it; no line is a PASS.
//!
//! ```text
//! AnnualModelCatalogV1 { edition, activation_daa, previous_catalog_root, models: [ApprovedModelV1] }
//! ApprovedModelV1      { class_id, artifact_root, kernel_version, verification_plan_id }
//!
//! edition 0            = the genesis catalog: activation_daa 0, previous_catalog_root = 0^64
//! edition e ≥ 1        = one annual (soft / hard) fork: previous_catalog_root = root(edition e − 1), activation_daa strictly
//!                        increasing and at or above the tier fence; ADD-ONLY: a class id appears once over the whole chain, so no
//!                        edition overwrites, removes or re-roots a model (ADR-0175)
//! root(catalog)        = blake2b-512(key "misaka-palw/annual-model-catalog/v1", catalog_bytes_v1)            — §[`catalog_bytes_v1`]
//! active(h)            = the highest edition with activation_daa ≤ h                       (annual_catalog_active_at)
//! ApprovedModels(h)    = ⋃ models of editions 0..=active(h) — the root of active(h) commits to all of them through the chain
//! tier(c, r, h)        = 1000 ‰ if (c, r) ∈ ApprovedModels(h) else the registered 300 ‰        (model_tier_permille, through the
//!                        ONE tier API of `palw_model_tier_v1` — this module stores the list, it does not define a second tier)
//! ```
//!
//! **What is consensus and what is not** (RFC-0019 §3). Consensus reads three things only: the chain of catalogs a binary carries
//! (hashed into the identity / schedule / fork id like any fence value), each edition's activation DAA, and the post-ACTIVE rule
//! (the tier the allocation roll reads). The operational lifecycle `PROPOSED → TESTED → PREPARED → LOCKED_IN → ACTIVE` (or
//! `ABORTED`), readiness measurements, release manifests and signatures are tooling ([`PalwCatalogLifecycleV1`] names them so the
//! manifest and the tools spell them one way). **No vote, no stake-weighted readiness signal and no version self-report is ever an
//! input of a rule here.** A binary carries an edition from its stable release (PREPARED) on; a node derives only "scheduled" or
//! "active" ([`PalwCatalogBinaryStateV1`]).
//!
//! **The accepted-DAA boundary rule** (RFC-0019 §6.4). The tier is read ONCE per allocation epoch, at the roll (the first chain block of
//! the epoch, `tick_bond_budget_v1`), and every block right (RFC-0016 V+) and coinbase share of that epoch comes from that one
//! snapshot. A claim's accepted DAA is the DAA of the chain block whose mergeset applies it — BLUE and RED merged blocks alike — so a
//! RED block mined below an edition's activation and merged above it is accepted exactly once, in the epoch of its accepting block,
//! against that epoch's single snapshot ([`palw_catalog_tier_reference_daa_v1`]). Claims accepted before the activation complete
//! under the rights and reward reserved at acceptance; nothing re-reads the tier for them.
//!
//! **Economic only** (PRINCIPLES §9.5). Membership changes the allocation weight `A_m = t_m · S̄_m^1.5` and nothing else: no
//! safe-frontier, settlement, rule-E, settled-anchor or weight privilege. Per-claim Gate C (`SecurityCertified`) applies to every
//! edition and every tier.

use std::collections::BTreeSet;

use borsh::{BorshDeserialize, BorshSerialize};

use crate::Hash64;
use crate::config::params::{ForkActivation, Params};

/// The catalog root's domain (blake2b-512 key, 35 bytes).
pub const PALW_ANNUAL_CATALOG_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/annual-model-catalog/v1";
/// The canonical encoding's version, the first two bytes of [`catalog_bytes_v1`].
pub const PALW_ANNUAL_CATALOG_ENCODING_VERSION_V1: u16 = 1;
/// The most models one edition may add (a bound on the encoding and on every lookup, not a policy target).
pub const PALW_ANNUAL_CATALOG_MAX_MODELS_PER_EDITION_V1: usize = 1_024;
/// The most editions a chain may hold (64 annual forks; a bound, not a plan).
pub const PALW_ANNUAL_CATALOG_MAX_EDITIONS_V1: usize = 64;
/// **The highest kernel version this build implements for a catalog entry** (ADR-0172 §3). `0` names the class's own registered
/// binding (a legacy profile or a TIR class: the entry adds no kernel requirement). A catalog naming a later version is refused at
/// startup: such an edition needs a binary that implements the kernel, which is a coordinated, fingerprinted upgrade — never a list
/// edit and never "skip what this build does not know".
pub const PALW_ANNUAL_CATALOG_KERNEL_VERSION_MAX_V1: u16 = 1;

/// **One catalog entry** — a registered class id, its fixed artifact root, the kernel version the entry requires and the
/// verification plan it names (`0^64` = the class's own registered plan). Membership is decided by `(class_id, artifact_root)`;
/// the other two are commitments an operator reads (and the root binds), not inputs of the tier.
///
/// **Which root an edition names** (audit C-06): the class's registered `artifact_root`, or — for a TIR class only — the root of
/// the `PALWART3` container the class bound on chain (tag 175), which `palw_model_tier_at_v1` reaches through the recorded binding.
/// A non-TIR class (A16, PALWQ361) has no on-chain container door, so an entry naming its container root never matches: such an
/// entry must name the registered root (see `palw_genesis8_edition0_v1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ApprovedModelV1 {
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub kernel_version: u16,
    pub verification_plan_id: Hash64,
}

impl ApprovedModelV1 {
    /// An entry with the class's own kernel binding and plan (kernel version 0, plan `0^64`).
    pub fn of(class_id: Hash64, artifact_root: Hash64) -> Self {
        Self { class_id, artifact_root, kernel_version: 0, verification_plan_id: Hash64::default() }
    }
}

/// **One edition of the catalog** — edition 0 is the genesis catalog; edition `e ≥ 1` is one annual fork's additions.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AnnualModelCatalogV1 {
    pub edition: u32,
    pub activation_daa: u64,
    pub previous_catalog_root: Hash64,
    /// The models this edition ADDS, strictly ascending by `class_id` (the canonical order the root is taken in).
    pub models: Vec<ApprovedModelV1>,
}

/// **The canonical bytes** of one catalog (version 1). Every integer little-endian, every hash its 64 raw bytes:
///
/// ```text
/// le16(1) ‖ le32(edition) ‖ le64(activation_daa) ‖ previous_catalog_root[64] ‖ le32(n)
///   ‖ n × ( class_id[64] ‖ artifact_root[64] ‖ le16(kernel_version) ‖ verification_plan_id[64] )   strictly ascending class_id
/// ```
///
/// Written by hand rather than through borsh so the release tooling (`scripts/release/misaka-release-manifest.py`) recomputes the
/// same bytes from JSON with the standard library alone. The order is the stored order: [`AnnualModelCatalogV1::validate_shape`]
/// refuses a catalog whose models are not strictly ascending, so one model set has one encoding.
pub fn catalog_bytes_v1(catalog: &AnnualModelCatalogV1) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + 4 + 8 + 64 + 4 + catalog.models.len() * (64 + 64 + 2 + 64));
    out.extend_from_slice(&PALW_ANNUAL_CATALOG_ENCODING_VERSION_V1.to_le_bytes());
    out.extend_from_slice(&catalog.edition.to_le_bytes());
    out.extend_from_slice(&catalog.activation_daa.to_le_bytes());
    out.extend_from_slice(catalog.previous_catalog_root.as_byte_slice());
    out.extend_from_slice(&(catalog.models.len() as u32).to_le_bytes());
    for m in &catalog.models {
        out.extend_from_slice(m.class_id.as_byte_slice());
        out.extend_from_slice(m.artifact_root.as_byte_slice());
        out.extend_from_slice(&m.kernel_version.to_le_bytes());
        out.extend_from_slice(m.verification_plan_id.as_byte_slice());
    }
    out
}

impl AnnualModelCatalogV1 {
    /// **The genesis catalog (edition 0)** over `models`, sorted into the canonical order.
    pub fn genesis(mut models: Vec<ApprovedModelV1>) -> Self {
        models.sort_by(|a, b| a.class_id.cmp(&b.class_id));
        Self { edition: 0, activation_daa: 0, previous_catalog_root: Hash64::default(), models }
    }

    /// **`root(catalog)`** = blake2b-512 keyed by [`PALW_ANNUAL_CATALOG_ROOT_DOMAIN_V1`] over [`catalog_bytes_v1`]. Through
    /// `previous_catalog_root`, the root of edition `e` commits to every edition before it.
    pub fn root(&self) -> Hash64 {
        let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_ANNUAL_CATALOG_ROOT_DOMAIN_V1).to_state();
        state.update(&catalog_bytes_v1(self));
        let mut out = [0u8; 64];
        out.copy_from_slice(state.finalize().as_bytes());
        Hash64::from_bytes(out)
    }

    /// Whether this edition itself adds `(class_id, artifact_root)` — the root must match: an entry never re-roots a class.
    pub fn adds(&self, class_id: &Hash64, artifact_root: &Hash64) -> bool {
        self.models.binary_search_by(|m| m.class_id.cmp(class_id)).ok().is_some_and(|i| self.models[i].artifact_root == *artifact_root)
    }

    /// **One edition's own refusals** (the chain's are in [`PalwAnnualCatalogChainV1::validate`]): bounded size, non-zero ids and
    /// roots, strictly ascending class ids (one encoding per set), a kernel version this build implements.
    pub fn validate_shape(&self) -> Result<(), &'static str> {
        if self.models.len() > PALW_ANNUAL_CATALOG_MAX_MODELS_PER_EDITION_V1 {
            return Err("an annual catalog edition adds at most PALW_ANNUAL_CATALOG_MAX_MODELS_PER_EDITION_V1 models");
        }
        for (i, m) in self.models.iter().enumerate() {
            if m.class_id == Hash64::default() || m.artifact_root == Hash64::default() {
                return Err("an annual catalog entry names a non-zero class id and artifact root");
            }
            if m.kernel_version > PALW_ANNUAL_CATALOG_KERNEL_VERSION_MAX_V1 {
                return Err(
                    "an annual catalog entry names a kernel version this build does not implement: that edition needs a binary that implements it (ADR-0172 §3, a coordinated upgrade)",
                );
            }
            if i > 0 && self.models[i - 1].class_id >= m.class_id {
                return Err("an annual catalog edition lists its models strictly ascending by class id (one canonical encoding)");
            }
        }
        Ok(())
    }
}

/// **The chain of catalogs a binary carries** — edition 0 first, then every LOCKED_IN annual edition in order.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PalwAnnualCatalogChainV1 {
    pub editions: Vec<AnnualModelCatalogV1>,
}

impl PalwAnnualCatalogChainV1 {
    /// A chain holding only the genesis catalog.
    pub fn genesis_only(models: Vec<ApprovedModelV1>) -> Self {
        Self { editions: vec![AnnualModelCatalogV1::genesis(models)] }
    }

    /// **Append the next annual edition** (edition number and previous root derived; models sorted). The result is not validated:
    /// call [`Self::validate`].
    pub fn with_edition(mut self, activation_daa: u64, mut models: Vec<ApprovedModelV1>) -> Self {
        models.sort_by(|a, b| a.class_id.cmp(&b.class_id));
        let (edition, previous_catalog_root) = match self.editions.last() {
            Some(last) => (last.edition.saturating_add(1), last.root()),
            None => (0, Hash64::default()),
        };
        self.editions.push(AnnualModelCatalogV1 { edition, activation_daa, previous_catalog_root, models });
        self
    }

    /// **The chain's refusals** — the rules RFC-0019 §5 states as MUST:
    ///
    /// * at least the genesis edition; at most [`PALW_ANNUAL_CATALOG_MAX_EDITIONS_V1`];
    /// * edition 0 is `edition 0`, `activation_daa 0`, `previous_catalog_root 0^64`;
    /// * edition `i ≥ 1` is numbered `i`, chains to `root(edition i − 1)`, adds at least one model, activates at a real height
    ///   strictly above the previous edition's (`u64::MAX` is not a height);
    /// * **ADD-ONLY:** a class id appears once over the whole chain — re-listing it, with its own root or another, is refused, so no
    ///   edition overwrites, removes or re-roots an existing model (ADR-0175);
    /// * every edition passes [`AnnualModelCatalogV1::validate_shape`].
    pub fn validate(&self) -> Result<(), &'static str> {
        let Some(genesis) = self.editions.first() else {
            return Err("an annual catalog chain starts with the genesis catalog (edition 0)");
        };
        if self.editions.len() > PALW_ANNUAL_CATALOG_MAX_EDITIONS_V1 {
            return Err("an annual catalog chain holds at most PALW_ANNUAL_CATALOG_MAX_EDITIONS_V1 editions");
        }
        if genesis.edition != 0 || genesis.activation_daa != 0 || genesis.previous_catalog_root != Hash64::default() {
            return Err("the genesis catalog is edition 0 at DAA 0 with a zero previous root");
        }
        let mut seen: BTreeSet<Hash64> = BTreeSet::new();
        for (i, catalog) in self.editions.iter().enumerate() {
            catalog.validate_shape()?;
            if i > 0 {
                let previous = &self.editions[i - 1];
                if catalog.edition as usize != i {
                    return Err("annual catalog editions are numbered 0, 1, 2, … without gaps");
                }
                if catalog.previous_catalog_root != previous.root() {
                    return Err("an annual catalog edition chains to the root of the edition before it");
                }
                if catalog.models.is_empty() {
                    return Err("an annual catalog edition adds at least one model (an empty edition is a fence without a rule)");
                }
                if catalog.activation_daa == u64::MAX || catalog.activation_daa <= previous.activation_daa {
                    return Err("annual catalog editions activate at real heights, strictly increasing");
                }
            }
            for m in &catalog.models {
                if !seen.insert(m.class_id) {
                    return Err(
                        "the annual catalog only ADDS: a class id is listed once over the whole chain, never re-listed or re-rooted (ADR-0175)",
                    );
                }
            }
        }
        Ok(())
    }

    /// **A release's chain must extend the previous release's as a prefix** — byte-identical editions, the same roots, then only new
    /// editions. A release that rewrites, drops or reorders an edition is refused (release tooling and RFC-0019 §8 call this).
    pub fn extends(&self, older: &PalwAnnualCatalogChainV1) -> Result<(), &'static str> {
        if older.editions.len() > self.editions.len() {
            return Err("a newer release drops an annual catalog edition the previous release carried");
        }
        for (old, new) in older.editions.iter().zip(self.editions.iter()) {
            if old != new {
                return Err("a newer release rewrites an annual catalog edition the previous release carried");
            }
        }
        Ok(())
    }

    /// **`annual_catalog_active_at(daa)`** — the highest edition with `activation_daa ≤ daa` (edition 0 at any height); `None`
    /// only for an empty (invalid) chain.
    pub fn active_at(&self, daa: u64) -> Option<&AnnualModelCatalogV1> {
        self.editions.iter().take_while(|c| c.activation_daa <= daa).last()
    }

    /// **The edition that added `(class_id, artifact_root)`, if it is in `ApprovedModels(daa)`.** The root must match.
    pub fn listed_at(&self, class_id: &Hash64, artifact_root: &Hash64, daa: u64) -> Option<&AnnualModelCatalogV1> {
        self.editions.iter().take_while(|c| c.activation_daa <= daa).find(|c| c.adds(class_id, artifact_root))
    }

    /// The root of the edition active at `daa` (`0^64` for an empty chain).
    pub fn root_at(&self, daa: u64) -> Hash64 {
        self.active_at(daa).map(|c| c.root()).unwrap_or_default()
    }

    /// The first edition strictly above `daa`, if this binary carries one.
    pub fn next_after(&self, daa: u64) -> Option<&AnnualModelCatalogV1> {
        self.editions.iter().find(|c| c.activation_daa > daa)
    }

    /// **The editions' activation heights (edition 1 onward)** — the sub-fences `for_each_fence` visits and the fork id gates on.
    /// Edition 0 is not a fence: it is the genesis.
    pub fn scheduled_activations(&self) -> Vec<u64> {
        self.editions.iter().skip(1).map(|c| c.activation_daa).filter(|&d| d != 0 && d != u64::MAX).collect()
    }

    /// **Identity normalisation** (RFC-0019 §7.2): after the identity visitor has rewritten every scheduled edition's height to
    /// `u64::MAX` ("not yet"), those editions leave the identity with their contents — so a build that carries the next edition and
    /// one that does not share `consensus_identity_id` until the edition activates. Edition 0 (DAA 0) stays.
    pub fn drop_unscheduled_for_identity(&mut self) {
        self.editions.retain(|c| c.activation_daa != u64::MAX);
    }

    /// The chain as the identity hashers write it: the count, then each edition's number, height and root (the root binds the
    /// models and the previous root).
    pub fn write_into(&self, h: &mut kaspa_hashes::ConsensusParamsId) {
        h.write((self.editions.len() as u64).to_le_bytes());
        for c in &self.editions {
            h.write(c.edition.to_le_bytes());
            h.write(c.activation_daa.to_le_bytes());
            h.write(c.root().as_byte_slice());
        }
    }
}

/// **`annual_catalog_active_at(chain, daa)`** — the user's name for [`PalwAnnualCatalogChainV1::active_at`].
pub fn annual_catalog_active_at(chain: &PalwAnnualCatalogChainV1, daa: u64) -> Option<&AnnualModelCatalogV1> {
    chain.active_at(daa)
}

/// **`model_tier_permille(class_id, artifact_root, accepted_daa)`** — 1000 if the catalog active at `accepted_daa` contains
/// `(class_id, artifact_root)`, else the registered coefficient (300 by design); 1000 for every model below the tier fence (A-2).
/// Composed from the ONE tier API ([`crate::palw_model_tier_v1::palw_model_tier_of_v1`] and
/// [`crate::palw_model_tier_v1::palw_model_tier_coefficient_permille_v1`]); `genesis_assembly` is the chain fact ADR-0179 keeps
/// (a class the genesis assembly registered is approved). No registry state (`Active` included) is an argument.
pub fn model_tier_permille(
    mirror: Option<&crate::palw_model_tier_v1::PalwModelTierMirrorV1>,
    class_id: &Hash64,
    artifact_root: &Hash64,
    genesis_assembly: bool,
    accepted_daa: u64,
) -> u32 {
    let tier = crate::palw_model_tier_v1::palw_model_tier_of_v1(mirror, class_id, artifact_root, genesis_assembly, accepted_daa);
    crate::palw_model_tier_v1::palw_model_tier_coefficient_permille_v1(mirror, tier, accepted_daa)
}

/// **The DAA a claim's tier is read at** (RFC-0019 §6.4): the start (roll DAA) of the allocation epoch containing the claim's
/// accepted DAA — the one snapshot its rights and reward came from — or the accepted DAA itself where no allocation epoch exists
/// (no allocation, so no tier is applied to it anywhere). `accepted_daa` is the accepting chain block's DAA for BLUE and RED merged
/// blocks alike (`PalwClaimStateV2::accepted_daa`), never the merged block's own header DAA, so one claim has one reference.
pub fn palw_catalog_tier_reference_daa_v1(accepted_daa: u64, allocation_epoch_roll_daa: Option<u64>) -> u64 {
    match allocation_epoch_roll_daa {
        Some(roll) if roll <= accepted_daa => roll,
        _ => accepted_daa,
    }
}

/// **The operational lifecycle of a catalog edition** (RFC-0019 §4) — release-process states that live in the release manifest and
/// the operators' tooling and never reach a consensus rule. A binary may carry an edition from `Prepared` on (the stable release
/// that schedules it); consensus cannot tell `Prepared` from `LockedIn` (the release owner's "no abort release after this point"),
/// and does not need to: a carried edition applies at its height in every binary that carries it. What a node can derive is only
/// [`PalwCatalogBinaryStateV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCatalogLifecycleV1 {
    Proposed,
    Tested,
    Prepared,
    LockedIn,
    Active,
    Aborted,
}

impl PalwCatalogLifecycleV1 {
    /// The one spelling RPC, the release manifest and the drills use.
    pub fn name(self) -> &'static str {
        match self {
            Self::Proposed => "PROPOSED",
            Self::Tested => "TESTED",
            Self::Prepared => "PREPARED",
            Self::LockedIn => "LOCKED_IN",
            Self::Active => "ACTIVE",
            Self::Aborted => "ABORTED",
        }
    }

    /// Parse the manifest spelling.
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "PROPOSED" => Self::Proposed,
            "TESTED" => Self::Tested,
            "PREPARED" => Self::Prepared,
            "LOCKED_IN" => Self::LockedIn,
            "ACTIVE" => Self::Active,
            "ABORTED" => Self::Aborted,
            _ => return None,
        })
    }

    /// Whether a binary may carry an edition in this state: from the stable release that schedules it (`Prepared`) on. `Proposed`
    /// and `Tested` editions exist only in drafts and test networks; an `Aborted` edition is in no release.
    pub fn may_ship_in_a_binary(self) -> bool {
        matches!(self, Self::Prepared | Self::LockedIn | Self::Active)
    }
}

/// **What a node can derive about an edition its binary carries** — scheduled (below its height) or active (at or past it). Nothing
/// else about the release process is visible to consensus, and nothing else is reported as if it were.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCatalogBinaryStateV1 {
    Scheduled,
    Active,
}

impl PalwCatalogBinaryStateV1 {
    /// The RPC spelling: `SCHEDULED` / `ACTIVE`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Scheduled => "SCHEDULED",
            Self::Active => "ACTIVE",
        }
    }
}

/// The state a carried edition reads at `tip_daa`: `Active` at or past its height, `Scheduled` below it.
pub fn palw_catalog_binary_state_v1(catalog: &AnnualModelCatalogV1, tip_daa: u64) -> PalwCatalogBinaryStateV1 {
    if catalog.activation_daa <= tip_daa { PalwCatalogBinaryStateV1::Active } else { PalwCatalogBinaryStateV1::Scheduled }
}

/// One edition as RPC reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwAnnualCatalogEditionReadV1 {
    pub edition: u32,
    pub activation_daa: u64,
    pub previous_root: Hash64,
    pub root: Hash64,
    pub models: u32,
    pub state: PalwCatalogBinaryStateV1,
}

/// **What a node reports about the catalog** (RPC `getPalwModelRegistry` v4 `annualCatalog`, the CLI's registry view): the editions
/// this binary carries, the edition and root in force at the tip, the next activation and its root. Readiness is NOT here: it is
/// measured off-chain (RFC-0019 §9) and no node field is an input of it, because a version self-report can be faked.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PalwAnnualCatalogStatusV1 {
    /// The tier fence is scheduled on this network (a real height).
    pub scheduled: bool,
    pub fence_daa: u64,
    /// The tier fence is in force at the tip.
    pub active: bool,
    pub tip_daa: u64,
    pub current_edition: u32,
    pub current_root: Hash64,
    pub next_edition: Option<u32>,
    pub next_activation_daa: Option<u64>,
    pub next_root: Option<Hash64>,
    pub editions: Vec<PalwAnnualCatalogEditionReadV1>,
    pub kernel_version_max: u16,
}

impl Params {
    /// **The catalog chain this binary carries**, where the tier fence is scheduled at a real height.
    pub fn palw_annual_catalog_chain_v1(&self) -> Option<&PalwAnnualCatalogChainV1> {
        self.palw_model_tier_v1.as_ref().filter(|f| f.activation != ForkActivation::never()).map(|f| &f.catalogs)
    }

    /// **The editions' activation heights the fork id gates on** (empty where the tier fence is absent or `never()`).
    pub fn palw_annual_catalog_activations_v1(&self) -> Vec<u64> {
        self.palw_annual_catalog_chain_v1().map(|c| c.scheduled_activations()).unwrap_or_default()
    }
}

/// **The node's catalog status at `tip_daa`** (pure: Params and a height).
pub fn palw_annual_catalog_status_v1(params: &Params, tip_daa: u64) -> PalwAnnualCatalogStatusV1 {
    let Some(fence) = params.palw_model_tier_v1.as_ref().filter(|f| f.activation != ForkActivation::never()) else {
        return PalwAnnualCatalogStatusV1 {
            tip_daa,
            kernel_version_max: PALW_ANNUAL_CATALOG_KERNEL_VERSION_MAX_V1,
            ..Default::default()
        };
    };
    let chain = &fence.catalogs;
    let current = chain.active_at(tip_daa);
    let next = chain.next_after(tip_daa);
    PalwAnnualCatalogStatusV1 {
        scheduled: true,
        fence_daa: fence.activation.daa_score(),
        active: fence.activation.is_active(tip_daa),
        tip_daa,
        current_edition: current.map(|c| c.edition).unwrap_or(0),
        current_root: current.map(|c| c.root()).unwrap_or_default(),
        next_edition: next.map(|c| c.edition),
        next_activation_daa: next.map(|c| c.activation_daa),
        next_root: next.map(|c| c.root()),
        editions: chain
            .editions
            .iter()
            .map(|c| PalwAnnualCatalogEditionReadV1 {
                edition: c.edition,
                activation_daa: c.activation_daa,
                previous_root: c.previous_catalog_root,
                root: c.root(),
                models: c.models.len() as u32,
                state: palw_catalog_binary_state_v1(c, tip_daa),
            })
            .collect(),
        kernel_version_max: PALW_ANNUAL_CATALOG_KERNEL_VERSION_MAX_V1,
    }
}

#[cfg(test)]
mod tests;
