//! **palw_model_registry** — moved out of `message.rs` (2026-10-13, a behaviour-preserving split: every item is the same and keeps its path through `pub use palw_model_registry::*`).

use super::*;

// ---- ADR-0088 Decision 12: the model registry -------------------------------------------------

/// ADR-0088 Decision 1: one line's row as the tip holds it. A founding line nothing touched is
/// synthesised from its class (`has_row: false`). `developer` / `maintainer` are the row's own
/// fields — `None` means "the owner" (Decision 6). The payout payloads are the bonds' as the
/// registry holds them at the same tip, `None` when the role names no bond or a bond the
/// registry no longer has.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwModelLine {
    /// 128-hex line id; the founding line's is its class id.
    pub line_id: String,
    pub class_id: String,
    pub has_row: bool,
    pub owner: Option<RpcTransactionOutpoint>,
    pub owner_payout_payload: Option<String>,
    pub developer: Option<RpcTransactionOutpoint>,
    pub developer_payout_payload: Option<String>,
    pub maintainer: Option<RpcTransactionOutpoint>,
    pub maintainer_payout_payload: Option<String>,
    /// The name as UTF-8 (lossy); `name_hex` is the exact bytes the chain holds.
    pub name: String,
    pub name_hex: String,
    pub founded_daa: u64,
    /// The current version's number.
    pub current: u32,
    /// Versions published as previews and not yet promoted or withdrawn.
    pub previews: Vec<u32>,
    /// The commit count: versions are dense and monotone, and the next one is this plus one.
    pub versions_published: u32,
    pub contributor_permille_of_leg: u32,
    /// `Active` or `Retired`.
    pub status: String,
    pub retired_daa: Option<u64>,
}

impl Serializer for RpcPalwModelLine {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.line_id, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(bool, &self.has_row, writer)?;
        serialize!(Option<RpcTransactionOutpoint>, &self.owner, writer)?;
        store!(Option<String>, &self.owner_payout_payload, writer)?;
        serialize!(Option<RpcTransactionOutpoint>, &self.developer, writer)?;
        store!(Option<String>, &self.developer_payout_payload, writer)?;
        serialize!(Option<RpcTransactionOutpoint>, &self.maintainer, writer)?;
        store!(Option<String>, &self.maintainer_payout_payload, writer)?;
        store!(String, &self.name, writer)?;
        store!(String, &self.name_hex, writer)?;
        store!(u64, &self.founded_daa, writer)?;
        store!(u32, &self.current, writer)?;
        store!(Vec<u32>, &self.previews, writer)?;
        store!(u32, &self.versions_published, writer)?;
        store!(u32, &self.contributor_permille_of_leg, writer)?;
        store!(String, &self.status, writer)?;
        store!(Option<u64>, &self.retired_daa, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwModelLine {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let line_id = load!(String, reader)?;
        let class_id = load!(String, reader)?;
        let has_row = load!(bool, reader)?;
        let owner = deserialize!(Option<RpcTransactionOutpoint>, reader)?;
        let owner_payout_payload = load!(Option<String>, reader)?;
        let developer = deserialize!(Option<RpcTransactionOutpoint>, reader)?;
        let developer_payout_payload = load!(Option<String>, reader)?;
        let maintainer = deserialize!(Option<RpcTransactionOutpoint>, reader)?;
        let maintainer_payout_payload = load!(Option<String>, reader)?;
        let name = load!(String, reader)?;
        let name_hex = load!(String, reader)?;
        let founded_daa = load!(u64, reader)?;
        let current = load!(u32, reader)?;
        let previews = load!(Vec<u32>, reader)?;
        let versions_published = load!(u32, reader)?;
        let contributor_permille_of_leg = load!(u32, reader)?;
        let status = load!(String, reader)?;
        let retired_daa = load!(Option<u64>, reader)?;
        Ok(Self {
            line_id,
            class_id,
            has_row,
            owner,
            owner_payout_payload,
            developer,
            developer_payout_payload,
            maintainer,
            maintainer_payout_payload,
            name,
            name_hex,
            founded_daa,
            current,
            previews,
            versions_published,
            contributor_permille_of_leg,
            status,
            retired_daa,
        })
    }
}

/// ADR-0088 Decision 12: `getPalwModelLine(line_id)`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelLineRequest {
    /// 128-hex line id — a class id names the class's founding line.
    pub line_id: String,
}

impl Serializer for GetPalwModelLineRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.line_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelLineRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let line_id = load!(String, reader)?;
        Ok(Self { line_id })
    }
}

/// **ADR-0101: what a client checks a provider's service descriptor against**, as one line's
/// facts at one height. The check itself (`palw_service_descriptor_check_v1`) runs in the client:
/// no consensus rule reads a descriptor and the chain holds no URL — what was missing was a node
/// that could answer the facts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwLineServiceFacts {
    /// The union of the grants this line's declaration has IN EFFECT at `tip_daa`: a lapsed
    /// promise offers nothing to serve.
    pub declared_grants: u32,
    /// The roots this line OWNS and that are in force — past ADR-0143's fence the index's answer,
    /// so a copy line cannot offer the class's own founding artifact as its own.
    pub roots: Vec<String>,
    /// The registered keys of the line's own owner, developer and maintainer bonds. A grant only
    /// the line's origin may provide is checked against these.
    pub origin_pubkeys: Vec<String>,
}

impl Serializer for RpcPalwLineServiceFacts {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u32, &self.declared_grants, writer)?;
        store!(Vec<String>, &self.roots, writer)?;
        store!(Vec<String>, &self.origin_pubkeys, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwLineServiceFacts {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let declared_grants = load!(u32, reader)?;
        let roots = load!(Vec<String>, reader)?;
        let origin_pubkeys = load!(Vec<String>, reader)?;
        Ok(Self { declared_grants, roots, origin_pubkeys })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelLineResponse {
    /// False when neither a line row nor a class of that id exists (or the chain is not
    /// ConsensusV2); `line` is then `None`.
    pub exists: bool,
    pub line_id: String,
    pub line: Option<RpcPalwModelLine>,
    /// The current version's root, when the node holds that version.
    pub current_root: Option<String>,
    /// Decision 3: the roots in force for the line's CLASS at `tip_daa` — every line's.
    pub roots_in_force: Vec<String>,
    pub tip_daa: u64,
    /// ADR-0095: what this line's positions grant, `None` where nothing was declared or the
    /// membership is not in force.
    pub benefits: Option<RpcPalwModelBenefits>,
    /// ADR-0101: the facts a client checks a provider's service descriptor against.
    pub service_facts: RpcPalwLineServiceFacts,
}

impl Serializer for GetPalwModelLineResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &3, writer)?;
        store!(bool, &self.exists, writer)?;
        store!(String, &self.line_id, writer)?;
        serialize!(Option<RpcPalwModelLine>, &self.line, writer)?;
        store!(Option<String>, &self.current_root, writer)?;
        store!(Vec<String>, &self.roots_in_force, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        serialize!(Option<RpcPalwModelBenefits>, &self.benefits, writer)?;
        serialize!(RpcPalwLineServiceFacts, &self.service_facts, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelLineResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        let exists = load!(bool, reader)?;
        let line_id = load!(String, reader)?;
        let line = deserialize!(Option<RpcPalwModelLine>, reader)?;
        let current_root = load!(Option<String>, reader)?;
        let roots_in_force = load!(Vec<String>, reader)?;
        let tip_daa = load!(u64, reader)?;
        // A version-1 peer wrote nothing here; a membership it never knew about is `None`, not a
        // read past the end of the frame.
        let benefits = if version >= 2 { deserialize!(Option<RpcPalwModelBenefits>, reader)? } else { None };
        // Likewise for a version-2 peer: a node that never answered the service facts reads as
        // empty ones, which a client correctly treats as "this node cannot tell me", not as "this
        // line declares nothing" — the two differ, and `exists` plus `benefits` say which.
        let service_facts =
            if version >= 3 { deserialize!(RpcPalwLineServiceFacts, reader)? } else { RpcPalwLineServiceFacts::default() };
        Ok(Self { exists, line_id, line, current_root, roots_in_force, tip_daa, benefits, service_facts })
    }
}

/// ADR-0095 §4.1: one tier of a line's membership declaration.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwModelBenefitTier {
    pub min_units: u64,
    /// The raw bitset, so a client that knows a newer set can still render it.
    pub grants: u32,
    /// The names of the bits this node knows, for a card that need not know the protocol.
    pub grant_names: Vec<String>,
    pub lead_daa: u64,
    pub min_hold_daa: u64,
    pub note: String,
}

impl Serializer for RpcPalwModelBenefitTier {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.min_units, writer)?;
        store!(u32, &self.grants, writer)?;
        store!(Vec<String>, &self.grant_names, writer)?;
        store!(u64, &self.lead_daa, writer)?;
        store!(u64, &self.min_hold_daa, writer)?;
        store!(String, &self.note, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwModelBenefitTier {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let min_units = load!(u64, reader)?;
        let grants = load!(u32, reader)?;
        let grant_names = load!(Vec<String>, reader)?;
        let lead_daa = load!(u64, reader)?;
        let min_hold_daa = load!(u64, reader)?;
        let note = load!(String, reader)?;
        Ok(Self { min_units, grants, grant_names, lead_daa, min_hold_daa, note })
    }
}

/// ADR-0095: what a line promises its holders, as a reader sees it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwModelBenefits {
    /// §4.6: the tiers GOVERNING at `tip_daa` — empty when the promise has lapsed.
    pub tiers: Vec<RpcPalwModelBenefitTier>,
    /// §4.7: a weakening waiting out its notice, and the height it lands.
    pub pending_tiers: Vec<RpcPalwModelBenefitTier>,
    pub pending_effective_daa: Option<u64>,
    pub cadence_daa: u64,
    pub expires_daa: u64,
    pub declared_daa: u64,
    /// §4.6: `expired`, `cadenceMissed`, or absent when the promise is live.
    pub lapsed: Option<String>,
    /// The height the lapse names: the expiry, or the height a version was due by.
    pub lapse_daa: Option<u64>,
    /// §4.4: the window the fold is enforcing on this line's version paths right now.
    pub enforced_lead_daa: u64,
}

impl Serializer for RpcPalwModelBenefits {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(Vec<RpcPalwModelBenefitTier>, &self.tiers, writer)?;
        serialize!(Vec<RpcPalwModelBenefitTier>, &self.pending_tiers, writer)?;
        store!(Option<u64>, &self.pending_effective_daa, writer)?;
        store!(u64, &self.cadence_daa, writer)?;
        store!(u64, &self.expires_daa, writer)?;
        store!(u64, &self.declared_daa, writer)?;
        store!(Option<String>, &self.lapsed, writer)?;
        store!(Option<u64>, &self.lapse_daa, writer)?;
        store!(u64, &self.enforced_lead_daa, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwModelBenefits {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let tiers = deserialize!(Vec<RpcPalwModelBenefitTier>, reader)?;
        let pending_tiers = deserialize!(Vec<RpcPalwModelBenefitTier>, reader)?;
        let pending_effective_daa = load!(Option<u64>, reader)?;
        let cadence_daa = load!(u64, reader)?;
        let expires_daa = load!(u64, reader)?;
        let declared_daa = load!(u64, reader)?;
        let lapsed = load!(Option<String>, reader)?;
        let lapse_daa = load!(Option<u64>, reader)?;
        let enforced_lead_daa = load!(u64, reader)?;
        Ok(Self {
            tiers,
            pending_tiers,
            pending_effective_daa,
            cadence_daa,
            expires_daa,
            declared_daa,
            lapsed,
            lapse_daa,
            enforced_lead_daa,
        })
    }
}

/// ADR-0088 Decision 2: one version's row. The four hashes are DECLARATIONS the chain recorded
/// and never read (Decision 2); the usage is what the fold counted (Decision 4).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwModelVersion {
    pub line_id: String,
    pub version: u32,
    pub root: String,
    pub parent: Option<u32>,
    /// The proposal this version adopted, if any.
    pub adopted_from: Option<String>,
    pub runtime_hash: Option<String>,
    pub dataset_commitment: Option<String>,
    pub training_config_hash: Option<String>,
    pub notes_hash: Option<String>,
    pub published_daa: u64,
    pub published_by: Option<RpcTransactionOutpoint>,
    /// `Current`, `Preview`, `Superseded` or `Withdrawn`.
    pub status: String,
    /// A superseded version's grace end: its root is in force while the DAA is below it.
    pub until_daa: Option<u64>,
    /// Whether the root was in force at the tip DAA the answer was read at.
    pub in_force: bool,
    pub attempt_claims: u64,
    pub fp_claims: u64,
    /// A decimal string: the fold counts leaves in a u128.
    pub work_leaves: String,
    pub first_used_daa: Option<u64>,
    pub last_used_daa: Option<u64>,
}

impl Serializer for RpcPalwModelVersion {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.line_id, writer)?;
        store!(u32, &self.version, writer)?;
        store!(String, &self.root, writer)?;
        store!(Option<u32>, &self.parent, writer)?;
        store!(Option<String>, &self.adopted_from, writer)?;
        store!(Option<String>, &self.runtime_hash, writer)?;
        store!(Option<String>, &self.dataset_commitment, writer)?;
        store!(Option<String>, &self.training_config_hash, writer)?;
        store!(Option<String>, &self.notes_hash, writer)?;
        store!(u64, &self.published_daa, writer)?;
        serialize!(Option<RpcTransactionOutpoint>, &self.published_by, writer)?;
        store!(String, &self.status, writer)?;
        store!(Option<u64>, &self.until_daa, writer)?;
        store!(bool, &self.in_force, writer)?;
        store!(u64, &self.attempt_claims, writer)?;
        store!(u64, &self.fp_claims, writer)?;
        store!(String, &self.work_leaves, writer)?;
        store!(Option<u64>, &self.first_used_daa, writer)?;
        store!(Option<u64>, &self.last_used_daa, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwModelVersion {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let line_id = load!(String, reader)?;
        let version = load!(u32, reader)?;
        let root = load!(String, reader)?;
        let parent = load!(Option<u32>, reader)?;
        let adopted_from = load!(Option<String>, reader)?;
        let runtime_hash = load!(Option<String>, reader)?;
        let dataset_commitment = load!(Option<String>, reader)?;
        let training_config_hash = load!(Option<String>, reader)?;
        let notes_hash = load!(Option<String>, reader)?;
        let published_daa = load!(u64, reader)?;
        let published_by = deserialize!(Option<RpcTransactionOutpoint>, reader)?;
        let status = load!(String, reader)?;
        let until_daa = load!(Option<u64>, reader)?;
        let in_force = load!(bool, reader)?;
        let attempt_claims = load!(u64, reader)?;
        let fp_claims = load!(u64, reader)?;
        let work_leaves = load!(String, reader)?;
        let first_used_daa = load!(Option<u64>, reader)?;
        let last_used_daa = load!(Option<u64>, reader)?;
        Ok(Self {
            line_id,
            version,
            root,
            parent,
            adopted_from,
            runtime_hash,
            dataset_commitment,
            training_config_hash,
            notes_hash,
            published_daa,
            published_by,
            status,
            until_daa,
            in_force,
            attempt_claims,
            fp_claims,
            work_leaves,
            first_used_daa,
            last_used_daa,
        })
    }
}

/// ADR-0088 Decision 5: one evaluation — a declaration, saying who declared it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwModelEvaluation {
    pub evaluator_id: String,
    pub score_permille: u32,
    pub report_hash: String,
    pub posted_daa: u64,
    /// The bond that posted it.
    pub by: RpcTransactionOutpoint,
    /// Posted by the line's developer or maintainer — the line's own word.
    pub is_lines_own: bool,
}

impl Serializer for RpcPalwModelEvaluation {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.evaluator_id, writer)?;
        store!(u32, &self.score_permille, writer)?;
        store!(String, &self.report_hash, writer)?;
        store!(u64, &self.posted_daa, writer)?;
        serialize!(RpcTransactionOutpoint, &self.by, writer)?;
        store!(bool, &self.is_lines_own, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwModelEvaluation {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let evaluator_id = load!(String, reader)?;
        let score_permille = load!(u32, reader)?;
        let report_hash = load!(String, reader)?;
        let posted_daa = load!(u64, reader)?;
        let by = deserialize!(RpcTransactionOutpoint, reader)?;
        let is_lines_own = load!(bool, reader)?;
        Ok(Self { evaluator_id, score_permille, report_hash, posted_daa, by, is_lines_own })
    }
}

/// ADR-0088 Decision 12: `getPalwModelVersion(line_id, version)`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelVersionRequest {
    pub line_id: String,
    pub version: u32,
}

impl Serializer for GetPalwModelVersionRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.line_id, writer)?;
        store!(u32, &self.version, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelVersionRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let line_id = load!(String, reader)?;
        let version = load!(u32, reader)?;
        Ok(Self { line_id, version })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelVersionResponse {
    /// False when the node holds no such row — never published, or evicted past the history
    /// window (the explorer keeps the whole history); `version` is then `None`.
    pub exists: bool,
    pub line_id: String,
    pub version_number: u32,
    pub version: Option<RpcPalwModelVersion>,
    pub evaluations: Vec<RpcPalwModelEvaluation>,
    pub tip_daa: u64,
}

impl Serializer for GetPalwModelVersionResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.exists, writer)?;
        store!(String, &self.line_id, writer)?;
        store!(u32, &self.version_number, writer)?;
        serialize!(Option<RpcPalwModelVersion>, &self.version, writer)?;
        serialize!(Vec<RpcPalwModelEvaluation>, &self.evaluations, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelVersionResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let exists = load!(bool, reader)?;
        let line_id = load!(String, reader)?;
        let version_number = load!(u32, reader)?;
        let version = deserialize!(Option<RpcPalwModelVersion>, reader)?;
        let evaluations = deserialize!(Vec<RpcPalwModelEvaluation>, reader)?;
        let tip_daa = load!(u64, reader)?;
        Ok(Self { exists, line_id, version_number, version, evaluations, tip_daa })
    }
}

/// ADR-0088 Decision 12: `getPalwModelLines(class_id)`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelLinesRequest {
    /// 128-hex class id.
    pub class_id: String,
}

impl Serializer for GetPalwModelLinesRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelLinesRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let class_id = load!(String, reader)?;
        Ok(Self { class_id })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelLinesResponse {
    /// False when the class is not registered (or the chain is not ConsensusV2).
    pub exists: bool,
    pub class_id: String,
    /// The founding line first (synthesised when it has no row), then the others in id order.
    pub lines: Vec<RpcPalwModelLine>,
}

impl Serializer for GetPalwModelLinesResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.exists, writer)?;
        store!(String, &self.class_id, writer)?;
        serialize!(Vec<RpcPalwModelLine>, &self.lines, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelLinesResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let exists = load!(bool, reader)?;
        let class_id = load!(String, reader)?;
        let lines = deserialize!(Vec<RpcPalwModelLine>, reader)?;
        Ok(Self { exists, class_id, lines })
    }
}

/// ADR-0088 Decision 7: one proposal — a root and a note from any bond, adopted or not.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwModelProposal {
    pub proposal_id: String,
    pub line_id: String,
    pub root: String,
    pub note_hash: String,
    pub by: RpcTransactionOutpoint,
    pub posted_daa: u64,
    /// The version that adopted it, once one did.
    pub adopted_in: Option<u32>,
}

impl Serializer for RpcPalwModelProposal {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.proposal_id, writer)?;
        store!(String, &self.line_id, writer)?;
        store!(String, &self.root, writer)?;
        store!(String, &self.note_hash, writer)?;
        serialize!(RpcTransactionOutpoint, &self.by, writer)?;
        store!(u64, &self.posted_daa, writer)?;
        store!(Option<u32>, &self.adopted_in, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwModelProposal {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let proposal_id = load!(String, reader)?;
        let line_id = load!(String, reader)?;
        let root = load!(String, reader)?;
        let note_hash = load!(String, reader)?;
        let by = deserialize!(RpcTransactionOutpoint, reader)?;
        let posted_daa = load!(u64, reader)?;
        let adopted_in = load!(Option<u32>, reader)?;
        Ok(Self { proposal_id, line_id, root, note_hash, by, posted_daa, adopted_in })
    }
}

/// ADR-0088 Decision 12: `getPalwModelProposals(line_id)`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelProposalsRequest {
    pub line_id: String,
}

impl Serializer for GetPalwModelProposalsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.line_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelProposalsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let line_id = load!(String, reader)?;
        Ok(Self { line_id })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelProposalsResponse {
    /// False when the line does not exist (or the chain is not ConsensusV2).
    pub exists: bool,
    pub line_id: String,
    pub proposals: Vec<RpcPalwModelProposal>,
}

impl Serializer for GetPalwModelProposalsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.exists, writer)?;
        store!(String, &self.line_id, writer)?;
        serialize!(Vec<RpcPalwModelProposal>, &self.proposals, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelProposalsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let exists = load!(bool, reader)?;
        let line_id = load!(String, reader)?;
        let proposals = deserialize!(Vec<RpcPalwModelProposal>, reader)?;
        Ok(Self { exists, line_id, proposals })
    }
}
