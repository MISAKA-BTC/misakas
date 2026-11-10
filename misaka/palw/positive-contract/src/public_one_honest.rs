//! Shared contracts for the public-materials profile. These values carry no
//! native admission, availability, verdict, or settlement authority.

use crate::ContractError;
use crate::identity::{ChainScope, Digest, PUBKEY_BYTES, PrincipalId, SIGNATURE_BYTES, hash, principal, verify_signature};
use crate::reservations::RemainingCapacity;
use crate::rows::AcceptedOrigin;
use borsh::{BorshDeserialize, BorshSerialize};

pub const ORIGINAL_SIGN_CONTEXT: &[u8] = b"misaka/palw/public-original/v1";
pub const FULL_REPLAY_SIGN_CONTEXT: &[u8] = b"misaka/palw/public-full-replay/v1";
pub const MANIFEST_REQUEST_SIGN_CONTEXT: &[u8] = b"misaka/palw/public-manifest-request/v1";
pub const MANIFEST_RESPONSE_SIGN_CONTEXT: &[u8] = b"misaka/palw/public-manifest-response/v1";
const ACCEPTED_ORIGIN_WIRE_BYTES: usize = 3 * 64 + 2 * 4 + 2 * 8;

/// Binds the native supply obligation and its exact limits into the
/// original's existing signed service-contract field.
pub fn public_original_service_contract_id_v1(obligation: Digest, budget: &crate::messages::AggregateBudget) -> Digest {
    hash(b"misaka/palw/public-original-service/v1", &(obligation, budget))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct OriginalId(pub Digest);

impl OriginalId {
    /// Scope of a replay approval over the complete native relation plan.
    /// The original identity commits to its rules, execution and manifest.
    pub fn full_replay_scope(self, relation_plan: Digest) -> Digest {
        hash(b"misaka/palw/public-replay-scope/v1", &(self, relation_plan))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MaterialId(pub Digest);

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CaseId(pub Digest);

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PanelBond {
    pub transaction: Digest,
    pub index: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PublicDrawMemberV1 {
    pub bond: PanelBond,
    pub principal: PrincipalId,
    pub rescue_reservation: Digest,
}

/// Compact identity of the ordered membership and its protected allocations.
pub fn public_draw_members_digest_v1(members: &[PublicDrawMemberV1]) -> Digest {
    hash(b"misaka/palw/public-draw-members/v1", &members)
}

/// Identity of a draw whose beacon and membership were resolved by consensus.
pub fn public_draw_id_v1(
    original: OriginalId,
    admission_carrier: Digest,
    block: Digest,
    beacon: Digest,
    full_scope: Digest,
    members: Digest,
) -> Digest {
    hash(b"misaka/palw/public-draw-id/v1", &(original, admission_carrier, block, beacon, full_scope, members))
}

/// The canonical identity native admission must resolve from the draw or
/// from admitted outsider eligibility before using a request's case ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Claimant {
    DrawnBond(PanelBond),
    Outsider(PrincipalId),
}

/// Fixed-width native representation of the two claimant kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct ManifestClaimantRecordV1 {
    drawn: bool,
    identity: Digest,
    index: u32,
}

impl ManifestClaimantRecordV1 {
    fn new(claimant: Claimant) -> Self {
        match claimant {
            Claimant::DrawnBond(bond) => Self { drawn: true, identity: bond.transaction, index: bond.index },
            Claimant::Outsider(principal) => Self { drawn: false, identity: principal.0, index: 0 },
        }
    }

    fn decode(self) -> Result<Claimant, ContractError> {
        match (self.drawn, self.index) {
            (true, index) => Ok(Claimant::DrawnBond(PanelBond { transaction: self.identity, index })),
            (false, 0) => Ok(Claimant::Outsider(PrincipalId(self.identity))),
            (false, _) => Err(ContractError::NonCanonical),
        }
    }
}

fn manifest_case_id(original: OriginalId, claimant: Claimant, material: MaterialId, scope: Digest) -> CaseId {
    CaseId(hash(b"misaka/palw/public-manifest-case/v1", &(original, claimant, material, scope)))
}

/// The version of this signed body is bound by its domain. The ruleset in
/// scope binds the execution profile and its parameters independently of any
/// future wire tag or activation height.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct OriginalCommit {
    pub scope: ChainScope,
    pub job: Digest,
    pub execution: Digest,
    pub miner: PrincipalId,
    pub registered_class: Digest,
    pub artifact: Digest,
    pub program: Digest,
    pub prompt: Digest,
    pub input_tokens: Digest,
    pub tokenizer: Digest,
    pub execution_rules: Digest,
    pub generation_seed: [u8; 32],
    pub initial_state: Digest,
    pub dependencies: Digest,
    pub prefill: u32,
    pub decode: u32,
    pub max_context: u32,
    pub output: Digest,
    pub trace_root: Digest,
    pub manifest_root: Digest,
    pub service_contract: Digest,
}

/// Fixed-size transport of the original and its detached Miner signature.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SignedPublicOriginalV1 {
    pub original: OriginalCommit,
    pub key: [u8; PUBKEY_BYTES],
    pub signature: [u8; SIGNATURE_BYTES],
}

impl SignedPublicOriginalV1 {
    pub const WIRE_BYTES: usize = 19 * 64 + 32 + 3 * 4 + PUBKEY_BYTES + SIGNATURE_BYTES;

    /// Identity of the literal carrier, including the key and signature.
    pub fn carrier_digest(original: &OriginalCommit, key: &[u8; PUBKEY_BYTES], signature: &[u8; SIGNATURE_BYTES]) -> Digest {
        hash(b"misaka/palw/public-original-carrier/v1", &(original, key, signature))
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ContractError> {
        if bytes.len() != Self::WIRE_BYTES {
            return Err(ContractError::NonCanonical);
        }
        borsh::from_slice(bytes).map_err(|_| ContractError::NonCanonical)
    }
}

impl OriginalCommit {
    /// Validate and encode the manifest before binding its root for signing.
    pub fn bind_manifest(
        &mut self,
        manifest: &ManifestV1,
        scope_limits: &[ManifestScopeRules; MaterialScope::COUNT],
        total_limit: u64,
        max_manifest_bytes: u32,
    ) -> Result<Vec<u8>, ContractError> {
        let wire_bytes = ManifestV1::fetch_size(max_manifest_bytes)?;
        manifest.check(self, scope_limits, total_limit)?;
        let bytes = borsh::to_vec(manifest).map_err(|_| ContractError::NonCanonical)?;
        if bytes.len() != wire_bytes as usize {
            return Err(ContractError::NonCanonical);
        }
        self.manifest_root = manifest_root(self.scope.ruleset, &bytes, max_manifest_bytes)?;
        Ok(bytes)
    }

    pub fn id(&self) -> OriginalId {
        OriginalId(hash(b"misaka/palw/public-original-id/v1", self))
    }

    pub fn authenticate(&self, key: &[u8; PUBKEY_BYTES], signature: &[u8; SIGNATURE_BYTES]) -> Result<(), ContractError> {
        let positions =
            self.prefill.checked_add(self.decode.checked_sub(1).ok_or(ContractError::Range)?).ok_or(ContractError::Range)?;
        if self.prefill == 0 || positions > self.max_context {
            return Err(ContractError::Range);
        }
        authenticate_actor(self.miner, key, signature, self.id().0, ORIGINAL_SIGN_CONTEXT)
    }

    /// Authenticate the bounded bytes obtained for this original. The caller
    /// resolves the byte cap from the accepted profile before reading or
    /// allocating the body. Schema and service-opportunity checks are separate.
    pub fn authenticate_manifest<'a>(&self, bytes: &'a [u8], max_bytes: u32) -> Result<AuthenticatedManifest<'a>, ContractError> {
        if manifest_root(self.scope.ruleset, bytes, max_bytes)? != self.manifest_root {
            return Err(ContractError::Identity);
        }
        Ok(AuthenticatedManifest { id: self.manifest_material_id(), bytes })
    }

    pub fn manifest_material_id(&self) -> MaterialId {
        MaterialId(hash(b"misaka/palw/public-manifest-material/v1", &(self.scope.ruleset, self.manifest_root)))
    }
}

/// The commitment the Miner puts in its original over canonical manifest
/// bytes. Rulesets cannot reinterpret the same bytes under this root.
pub fn manifest_root(ruleset: Digest, bytes: &[u8], max_bytes: u32) -> Result<Digest, ContractError> {
    if bytes.is_empty() || bytes.len() > max_bytes as usize {
        return Err(ContractError::Range);
    }
    Ok(hash(b"misaka/palw/public-manifest-root/v1", &(ruleset, bytes)))
}

/// A byte identity check. It carries no observation of when or to whom the
/// material was made available and confers no inspection rights by itself.
pub struct AuthenticatedManifest<'a> {
    id: MaterialId,
    bytes: &'a [u8],
}

impl<'a> AuthenticatedManifest<'a> {
    pub fn id(&self) -> MaterialId {
        self.id
    }

    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// Interpret the fixed-width manifest only against the same original.
    /// Size limits come from the accepted profile, including the total
    /// retrieval reservation across all scopes.
    pub fn decode(
        &self,
        original: &OriginalCommit,
        scope_limits: &[ManifestScopeRules; MaterialScope::COUNT],
        total_limit: u64,
    ) -> Result<CheckedManifestV1, ContractError> {
        if self.id != original.manifest_material_id() {
            return Err(ContractError::Scope);
        }
        if self.bytes.len() != ManifestV1::WIRE_BYTES as usize {
            return Err(ContractError::NonCanonical);
        }
        let manifest: ManifestV1 = borsh::from_slice(self.bytes).map_err(|_| ContractError::NonCanonical)?;
        manifest.check(original, scope_limits, total_limit)?;
        Ok(CheckedManifestV1 { original: original.id(), ruleset: original.scope.ruleset, manifest })
    }
}

/// Fixed entry order for this manifest format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum MaterialScope {
    Artifact,
    Program,
    Prompt,
    InputTokens,
    Tokenizer,
    ExecutionRules,
    InitialState,
    Dependencies,
    Trace,
    Output,
}

impl MaterialScope {
    pub const COUNT: usize = 10;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ManifestScopeRules {
    pub codec: Digest,
    pub max_bytes: u64,
    pub chunk_bytes: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ManifestEntryV1 {
    pub subject: Digest,
    pub content_root: Digest,
    pub codec: Digest,
    pub total_bytes: u64,
    pub chunk_bytes: u32,
}

impl ManifestEntryV1 {
    /// Canonical half-open byte range for a chunk. The final chunk may be
    /// shorter. A requester cannot move the boundary or ask for a range past
    /// the committed end.
    pub fn chunk_range(&self, index: u64) -> Result<std::ops::Range<u64>, ContractError> {
        let width = u64::from(self.chunk_bytes);
        let start = index.checked_mul(width).ok_or(ContractError::Range)?;
        if width == 0 || start >= self.total_bytes {
            return Err(ContractError::Range);
        }
        let len = width.min(self.total_bytes - start);
        Ok(start..start + len)
    }

    /// Content identity for one canonical range. Case IDs, claimant names
    /// and deadlines do not affect byte sharing. The opening for these bytes
    /// still has to authenticate against the entry's content root.
    pub fn chunk_material_id(&self, ruleset: Digest, index: u64) -> Result<MaterialId, ContractError> {
        let range = self.chunk_range(index)?;
        Ok(MaterialId(hash(
            b"misaka/palw/public-material-chunk/v1",
            &(ruleset, self.codec, self.content_root, self.total_bytes, range.start, range.end),
        )))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ManifestV1 {
    pub service_contract: Digest,
    pub entries: [ManifestEntryV1; MaterialScope::COUNT],
}

impl ManifestV1 {
    // Borsh fixed arrays have no length prefix. Each entry holds three
    // digests, one u64 and one u32; the header holds the service digest.
    pub const WIRE_BYTES: u32 = 64 + MaterialScope::COUNT as u32 * (3 * 64 + 8 + 4);

    /// Reserve the exact manifest body length before any provider read. A
    /// profile whose byte limit cannot carry this format cannot request it.
    pub fn fetch_size(max_manifest_bytes: u32) -> Result<u32, ContractError> {
        if max_manifest_bytes < Self::WIRE_BYTES {
            return Err(ContractError::Range);
        }
        Ok(Self::WIRE_BYTES)
    }

    pub fn entry(&self, scope: MaterialScope) -> &ManifestEntryV1 {
        &self.entries[scope as usize]
    }

    fn check(
        &self,
        original: &OriginalCommit,
        scope_limits: &[ManifestScopeRules; MaterialScope::COUNT],
        total_limit: u64,
    ) -> Result<(), ContractError> {
        let subjects = [
            original.artifact,
            original.program,
            original.prompt,
            original.input_tokens,
            original.tokenizer,
            original.execution_rules,
            original.initial_state,
            original.dependencies,
            original.trace_root,
            original.output,
        ];
        if self.service_contract != original.service_contract {
            return Err(ContractError::Scope);
        }
        let mut total = 0u64;
        for ((entry, subject), limit) in self.entries.iter().zip(subjects).zip(scope_limits) {
            if entry.subject != subject || entry.codec != limit.codec {
                return Err(ContractError::Scope);
            }
            // Empty collections still carry their canonical encoded body.
            if entry.chunk_bytes == 0 || entry.chunk_bytes != limit.chunk_bytes || entry.total_bytes == 0 {
                return Err(ContractError::NonCanonical);
            }
            if entry.total_bytes > limit.max_bytes {
                return Err(ContractError::Budget);
            }
            total = total.checked_add(entry.total_bytes).ok_or(ContractError::Budget)?;
        }
        if total > total_limit {
            return Err(ContractError::Budget);
        }
        Ok(())
    }
}

/// Descriptors whose subjects and budgets were checked against one original.
/// A chunk's opening must still be verified by its codec before use.
#[derive(Debug, PartialEq, Eq)]
pub struct CheckedManifestV1 {
    original: OriginalId,
    ruleset: Digest,
    manifest: ManifestV1,
}

impl CheckedManifestV1 {
    pub fn open_prompt_text<'a>(&self, bytes: &'a [u8]) -> Result<&'a str, ContractError> {
        let entry = self.entry(MaterialScope::Prompt);
        if bytes.len() as u64 != entry.total_bytes || entry.total_bytes > u64::from(entry.chunk_bytes) {
            return Err(ContractError::Range);
        }
        let text = std::str::from_utf8(bytes).map_err(|_| ContractError::NonCanonical)?;
        let rules = ManifestScopeRules { codec: entry.codec, max_bytes: entry.total_bytes, chunk_bytes: entry.chunk_bytes };
        if prompt_text_material_v1(self.ruleset, text, rules)? != *entry {
            return Err(ContractError::Identity);
        }
        Ok(text)
    }

    pub fn original(&self) -> OriginalId {
        self.original
    }

    pub fn entry(&self, scope: MaterialScope) -> &ManifestEntryV1 {
        self.manifest.entry(scope)
    }

    pub fn chunk(&self, scope: MaterialScope, index: u64) -> Result<ManifestChunkV1, ContractError> {
        let entry = self.entry(scope);
        Ok(ManifestChunkV1 {
            id: entry.chunk_material_id(self.ruleset, index)?,
            content_root: entry.content_root,
            codec: entry.codec,
            range: entry.chunk_range(index)?,
        })
    }
}

/// One complete canonical TIR v1 program, opened by its native graph IR hash.
pub fn tir_program_material_codec_v1() -> Digest {
    hash(b"misaka/palw/material-codec/v1", &b"tir-program-v1/native-graph-ir/whole")
}

/// One complete generated token sequence under the native CoreV1 output rule.
pub fn tir_output_material_codec_v1() -> Digest {
    hash(b"misaka/palw/material-codec/v1", &b"tir-output/core-v1/u32-le/whole")
}

pub fn prompt_text_material_codec_v1() -> Digest {
    hash(b"misaka/palw/material-codec/v1", &b"prompt-text/utf8-exact/whole")
}

pub fn tokenizer_json_material_codec_v1() -> Digest {
    hash(b"misaka/palw/material-codec/v1", &b"tokenizer-json/native-raw/whole")
}

/// Preserve the exact UTF-8 bytes independently of token IDs. The relation
/// between these bytes and the token sequence is checked by the profile.
pub fn prompt_text_material_v1(ruleset: Digest, text: &str, rules: ManifestScopeRules) -> Result<ManifestEntryV1, ContractError> {
    if rules.codec != prompt_text_material_codec_v1() {
        return Err(ContractError::UnsupportedProfile);
    }
    let total_bytes = text.len() as u64;
    if total_bytes == 0 || total_bytes > rules.max_bytes || total_bytes > u64::from(rules.chunk_bytes) {
        return Err(ContractError::Budget);
    }
    let subject = hash(b"misaka/palw/public-prompt-text/v1", &(ruleset, text.as_bytes()));
    Ok(ManifestEntryV1 { subject, content_root: subject, codec: rules.codec, total_bytes, chunk_bytes: rules.chunk_bytes })
}

#[derive(Debug, PartialEq, Eq)]
pub struct ManifestChunkV1 {
    pub id: MaterialId,
    pub content_root: Digest,
    pub codec: Digest,
    pub range: std::ops::Range<u64>,
}

/// A Panel member's assertion of complete replay under the rules bound by the
/// original. Native consumers must check that this member belongs to the
/// original's actual post-commit draw and independently enforce the Final gate.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FullReplayApproval {
    pub original: OriginalId,
    pub draw: Digest,
    pub bond: PanelBond,
    pub member: PrincipalId,
    pub full_scope: Digest,
}

/// Expected values from the original's native draw and execution profile.
/// One signer may own multiple bonds, so the bond is checked independently.
pub struct ReplayApprovalContext {
    pub original: OriginalId,
    pub draw: Digest,
    pub bond: PanelBond,
    pub member: PrincipalId,
    pub full_scope: Digest,
}

impl FullReplayApproval {
    pub fn signing_digest(&self) -> Digest {
        hash(b"misaka/palw/public-full-replay-id/v1", self)
    }

    pub fn authenticate(
        &self,
        context: &ReplayApprovalContext,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<(), ContractError> {
        self.check_subject(context)?;
        authenticate_actor(context.member, key, signature, self.signing_digest(), FULL_REPLAY_SIGN_CONTEXT)
    }

    fn check_subject(&self, context: &ReplayApprovalContext) -> Result<(), ContractError> {
        if self.original != context.original || self.draw != context.draw || self.full_scope != context.full_scope {
            return Err(ContractError::Scope);
        }
        if self.bond != context.bond || self.member != context.member {
            return Err(ContractError::Identity);
        }
        Ok(())
    }
}

/// Request the original's manifest itself, before a claimant knows any
/// chunk index or has obtained any execution data. This request covers the
/// canonical manifest entry point. The profile must supply its size cap and
/// the native rules for validating the inspection scope and reservation.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ManifestRequest {
    pub original: OriginalId,
    pub manifest_root: Digest,
    pub claimant: Claimant,
    pub signer: PrincipalId,
    pub inspection_scope: Digest,
    pub supply_obligation: Digest,
    pub collect_until: u64,
    pub reservation: Digest,
}

/// Bounded carrier for a first material request. The claimant kind is the
/// only field that changes its encoded length.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SignedManifestRequestV1 {
    pub request: ManifestRequest,
    pub key: [u8; PUBKEY_BYTES],
    pub signature: [u8; SIGNATURE_BYTES],
}

impl SignedManifestRequestV1 {
    const OUTSIDER_BYTES: usize = 7 * 64 + 1 + 8 + PUBKEY_BYTES + SIGNATURE_BYTES;
    pub const MAX_WIRE_BYTES: usize = Self::OUTSIDER_BYTES + 4;

    pub fn decode(bytes: &[u8]) -> Result<Self, ContractError> {
        if !matches!(bytes.len(), Self::OUTSIDER_BYTES | Self::MAX_WIRE_BYTES) {
            return Err(ContractError::NonCanonical);
        }
        borsh::from_slice(bytes).map_err(|_| ContractError::NonCanonical)
    }
}

/// Identity of the accepted profile limits used to interpret the manifest.
pub fn manifest_limits_id_v1(
    scope_limits: &[ManifestScopeRules; MaterialScope::COUNT],
    total_limit: u64,
    max_manifest_bytes: u32,
) -> Digest {
    hash(b"misaka/palw/public-manifest-limits/v1", &(*scope_limits, total_limit, max_manifest_bytes))
}

/// Expected values read by the consumer from the accepted original, native
/// eligibility, and this claimant's booked service opportunity. It must never
/// be constructed from the submitted request.
pub struct ManifestRequestContext<'a> {
    pub original: &'a OriginalCommit,
    pub claimant: Claimant,
    pub signer: PrincipalId,
    pub inspection_scope: Digest,
    pub supply_obligation: Digest,
    pub reservation: Digest,
    /// Limit from the accepted material profile.
    pub max_manifest_bytes: u32,
    pub manifest_limits: Digest,
    /// Body bytes still booked for this claimant and obligation. The native
    /// consumer accounts for the envelope, verification work and retention separately.
    pub manifest_body_bytes_reserved: u64,
    pub schedule: VerificationSchedule,
    pub origin: AcceptedOrigin,
}

/// An authenticated request ready for the native fold to install alongside
/// its reservation. Delivery of this one object does not close inspection.
#[derive(Debug, PartialEq, Eq)]
pub struct PendingManifestCase {
    id: CaseId,
    original: OriginalId,
    material: MaterialId,
    claimant: Claimant,
    inspection_scope: Digest,
    signer: PrincipalId,
    reservation: Digest,
    supply_obligation: Digest,
    request_digest: Digest,
    origin: AcceptedOrigin,
    max_manifest_bytes: u32,
    manifest_limits: Digest,
    schedule: VerificationSchedule,
}

/// A provider's response for one case. Bytes travel separately under the
/// fixed manifest limit and are identified here before interpretation.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SignedManifestResponseV1 {
    pub case: CaseId,
    pub supply_obligation: Digest,
    pub provider: PrincipalId,
    pub body_root: Digest,
}

impl SignedManifestResponseV1 {
    pub fn signing_digest(&self) -> Digest {
        hash(b"misaka/palw/public-manifest-response-id/v1", self)
    }
}

/// Signed response header, followed by the bounded manifest body in a
/// delivery packet. Keeping the body separate avoids allocation on decode.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ManifestResponseCarrierV1 {
    pub response: SignedManifestResponseV1,
    pub key: [u8; PUBKEY_BYTES],
    pub signature: [u8; SIGNATURE_BYTES],
}

impl ManifestResponseCarrierV1 {
    pub const HEADER_BYTES: usize = 4 * 64 + PUBKEY_BYTES + SIGNATURE_BYTES;
}

/// Durable identity for a pending case. Restore must reauthenticate the
/// signed request against its original canonical reservation and compare.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PendingManifestCaseRecordV1 {
    id: CaseId,
    original: OriginalId,
    material: MaterialId,
    claimant: ManifestClaimantRecordV1,
    inspection_scope: Digest,
    signer: PrincipalId,
    reservation: Digest,
    supply_obligation: Digest,
    request_digest: Digest,
    origin: AcceptedOrigin,
    max_manifest_bytes: u32,
    manifest_limits: Digest,
    material_ready_by: u64,
    filed_by: u64,
    hard: u64,
}

/// Distinguish unusable committed data from a packet that failed authentication.
/// The native court decides responsibility under the applicable profile.
#[derive(Debug, PartialEq, Eq)]
pub enum ManifestDeliveryV1 {
    Checked(CheckedManifestV1),
    CommittedButInvalid(ContractError),
    /// The obligated provider signed these exact bytes for this case, but
    /// they do not open the original manifest commitment.
    WrongMaterial,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ManifestResponseRecordV1 {
    original: OriginalId,
    case: CaseId,
    supply_obligation: Digest,
    response_digest: Digest,
    origin: AcceptedOrigin,
    limits: Digest,
    classification: ManifestDeliveryClassV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
enum ManifestDeliveryClassV1 {
    Checked,
    CommittedButInvalid,
    WrongMaterial,
}

#[derive(Debug, PartialEq, Eq)]
pub struct VerifiedManifestDeliveryV1 {
    record: ManifestResponseRecordV1,
    outcome: ManifestDeliveryV1,
}

impl VerifiedManifestDeliveryV1 {
    pub fn record(&self) -> ManifestResponseRecordV1 {
        self.record
    }

    pub fn into_outcome(self) -> ManifestDeliveryV1 {
        self.outcome
    }
}

impl ManifestResponseRecordV1 {
    pub const WIRE_BYTES: usize = 5 * 64 + ACCEPTED_ORIGIN_WIRE_BYTES + 1;
    pub const KEY_BYTES: usize = 2 * 64;

    pub fn key(&self) -> [u8; Self::KEY_BYTES] {
        let mut key = [0; Self::KEY_BYTES];
        key[..64].copy_from_slice(&self.case.0.0);
        key[64..].copy_from_slice(&self.response_digest.0);
        key
    }

    pub fn decode_row(key: &[u8], bytes: &[u8]) -> Result<Self, ContractError> {
        let row: Self = borsh::from_slice(bytes).map_err(|_| ContractError::NonCanonical)?;
        if key != row.key().as_slice() {
            return Err(ContractError::Identity);
        }
        Ok(row)
    }

    pub fn case(&self) -> CaseId {
        self.case
    }

    pub fn original(&self) -> OriginalId {
        self.original
    }

    pub fn origin(&self) -> AcceptedOrigin {
        self.origin
    }

    pub fn check_case(&self, pending: &PendingManifestCaseRecordV1) -> Result<(), ContractError> {
        if self.original != pending.original
            || self.case != pending.id
            || self.supply_obligation != pending.supply_obligation
            || self.limits != pending.manifest_limits
            || !(pending.origin.accepted_daa..=pending.material_ready_by).contains(&self.origin.accepted_daa)
        {
            return Err(ContractError::Scope);
        }
        require_accepted_after_v1(pending.origin, self.origin)
    }

    /// The caller re-verifies the original signed packet before this check.
    pub fn check_restore(&self, delivery: &VerifiedManifestDeliveryV1) -> Result<(), ContractError> {
        if *self != delivery.record() {
            return Err(ContractError::Scope);
        }
        Ok(())
    }
}

/// Compare two events already resolved from the same canonical accepted history.
pub fn require_accepted_after_v1(prior: AcceptedOrigin, later: AcceptedOrigin) -> Result<(), ContractError> {
    if later.accepted_daa < prior.accepted_daa
        || later.accepted_blue_score < prior.accepted_blue_score
        || (later.accepted_blue_score == prior.accepted_blue_score
            && (later.accepting_block != prior.accepting_block
                || later.accepted_daa != prior.accepted_daa
                || (later.accepted_tx_ordinal, later.event_ordinal) <= (prior.accepted_tx_ordinal, prior.event_ordinal)))
    {
        return Err(ContractError::Scope);
    }
    Ok(())
}

impl PendingManifestCaseRecordV1 {
    pub const WIRE_BYTES: usize = 10 * 64 + ACCEPTED_ORIGIN_WIRE_BYTES + 3 * 8 + 2 * 4 + 1;
    pub const KEY_BYTES: usize = 2 * 64;

    pub fn key(original: OriginalId, case: CaseId) -> [u8; Self::KEY_BYTES] {
        let mut key = [0; Self::KEY_BYTES];
        key[..64].copy_from_slice(&original.0.0);
        key[64..].copy_from_slice(&case.0.0);
        key
    }

    pub fn decode_row(key: &[u8], bytes: &[u8]) -> Result<Self, ContractError> {
        let row: Self = borsh::from_slice(bytes).map_err(|_| ContractError::NonCanonical)?;
        if key != Self::key(row.original, row.id).as_slice() {
            return Err(ContractError::Identity);
        }
        let claimant = row.claimant.decode()?;
        if row.id != manifest_case_id(row.original, claimant, row.material, row.inspection_scope)
            || matches!(claimant, Claimant::Outsider(principal) if principal != row.signer)
        {
            return Err(ContractError::Identity);
        }
        ManifestV1::fetch_size(row.max_manifest_bytes)?;
        Ok(row)
    }

    pub fn original(&self) -> OriginalId {
        self.original
    }

    pub fn signer(&self) -> PrincipalId {
        self.signer
    }

    pub fn material(&self) -> MaterialId {
        self.material
    }

    pub fn claimant(&self) -> Result<Claimant, ContractError> {
        self.claimant.decode()
    }

    pub fn inspection_scope(&self) -> Digest {
        self.inspection_scope
    }

    pub fn origin(&self) -> AcceptedOrigin {
        self.origin
    }

    pub fn check_admission(
        &self,
        accepted_at: u64,
        state_daa: u64,
        max_manifest_bytes: u32,
        manifest_limits: Digest,
        material_ready_by: u64,
        filed_by: u64,
    ) -> Result<(), ContractError> {
        let clock = OriginalClock::new(accepted_at)?;
        if !clock.accepts_first_notice(self.origin.accepted_daa)
            || self.origin.accepted_daa > state_daa
            || self.max_manifest_bytes != max_manifest_bytes
            || self.manifest_limits != manifest_limits
            || self.material_ready_by != material_ready_by
            || self.filed_by != filed_by
            || self.hard != clock.hard()
        {
            return Err(ContractError::Scope);
        }
        Ok(())
    }

    pub fn check_restore(&self, pending: &PendingManifestCase) -> Result<(), ContractError> {
        if *self != pending.record() {
            return Err(ContractError::Scope);
        }
        Ok(())
    }

    /// Classify an unresolved case after its admission link has been checked.
    pub fn unresolved_at(&self, now: u64) -> Result<MissingInspectionOutcome, ContractError> {
        if now < self.origin.accepted_daa {
            return Err(ContractError::Range);
        }
        Ok(unresolved_inspection_at(now, self.material_ready_by, self.hard))
    }
}

impl PendingManifestCase {
    /// Recover a delivery only by replaying its literal packet against the
    /// restored request and the canonical provider obligation.
    #[allow(clippy::too_many_arguments)]
    pub fn restore_provider_response_bytes(
        &self,
        record: &ManifestResponseRecordV1,
        packet: &[u8],
        expected_provider: PrincipalId,
        original: &OriginalCommit,
        origin: AcceptedOrigin,
        scope_limits: &[ManifestScopeRules; MaterialScope::COUNT],
        total_limit: u64,
    ) -> Result<VerifiedManifestDeliveryV1, ContractError> {
        record.check_case(&self.record())?;
        let delivery = self.verify_provider_response_bytes(packet, expected_provider, original, origin, scope_limits, total_limit)?;
        record.check_restore(&delivery)?;
        Ok(delivery)
    }

    /// Verify a raw delivery under the provider and profile resolved from
    /// the native obligation. A delivery record leaves inspection pending.
    #[allow(clippy::too_many_arguments)]
    pub fn verify_provider_response_bytes(
        &self,
        packet: &[u8],
        expected_provider: PrincipalId,
        original: &OriginalCommit,
        origin: AcceptedOrigin,
        scope_limits: &[ManifestScopeRules; MaterialScope::COUNT],
        total_limit: u64,
    ) -> Result<VerifiedManifestDeliveryV1, ContractError> {
        let (header, body) = packet.split_at_checked(ManifestResponseCarrierV1::HEADER_BYTES).ok_or(ContractError::NonCanonical)?;
        if body.len() > self.max_manifest_bytes as usize {
            return Err(ContractError::Range);
        }
        let carrier: ManifestResponseCarrierV1 = borsh::from_slice(header).map_err(|_| ContractError::NonCanonical)?;
        self.verify_provider_response(
            &carrier.response,
            expected_provider,
            original,
            origin,
            body,
            scope_limits,
            total_limit,
            &carrier.key,
            &carrier.signature,
        )
    }

    /// Validate the provider named by the actual native obligation.
    /// Neither the signature nor this response settles the case.
    #[allow(clippy::too_many_arguments)]
    pub fn verify_provider_response(
        &self,
        response: &SignedManifestResponseV1,
        expected_provider: PrincipalId,
        original: &OriginalCommit,
        origin: AcceptedOrigin,
        bytes: &[u8],
        scope_limits: &[ManifestScopeRules; MaterialScope::COUNT],
        total_limit: u64,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<VerifiedManifestDeliveryV1, ContractError> {
        self.check_delivery_context(original, origin.accepted_daa)?;
        require_accepted_after_v1(self.origin, origin)?;
        let limits = manifest_limits_id_v1(scope_limits, total_limit, self.max_manifest_bytes);
        if limits != self.manifest_limits {
            return Err(ContractError::Scope);
        }
        if response.case != self.id || response.supply_obligation != self.supply_obligation {
            return Err(ContractError::Scope);
        }
        if response.provider != expected_provider {
            return Err(ContractError::Identity);
        }
        let body_root = manifest_root(original.scope.ruleset, bytes, self.max_manifest_bytes)?;
        if body_root != response.body_root {
            return Err(ContractError::Identity);
        }
        let response_digest = response.signing_digest();
        authenticate_actor(expected_provider, key, signature, response_digest, MANIFEST_RESPONSE_SIGN_CONTEXT)?;
        let outcome = if body_root != original.manifest_root {
            ManifestDeliveryV1::WrongMaterial
        } else {
            let authenticated = AuthenticatedManifest { id: self.material, bytes };
            match authenticated.decode(original, scope_limits, total_limit) {
                Ok(manifest) => ManifestDeliveryV1::Checked(manifest),
                Err(error) => ManifestDeliveryV1::CommittedButInvalid(error),
            }
        };
        let classification = match &outcome {
            ManifestDeliveryV1::Checked(_) => ManifestDeliveryClassV1::Checked,
            ManifestDeliveryV1::CommittedButInvalid(_) => ManifestDeliveryClassV1::CommittedButInvalid,
            ManifestDeliveryV1::WrongMaterial => ManifestDeliveryClassV1::WrongMaterial,
        };
        Ok(VerifiedManifestDeliveryV1 {
            record: ManifestResponseRecordV1 {
                original: self.original,
                case: self.id,
                supply_obligation: response.supply_obligation,
                response_digest,
                origin,
                limits,
                classification,
            },
            outcome,
        })
    }

    pub fn record(&self) -> PendingManifestCaseRecordV1 {
        PendingManifestCaseRecordV1 {
            id: self.id,
            original: self.original,
            material: self.material,
            claimant: ManifestClaimantRecordV1::new(self.claimant),
            inspection_scope: self.inspection_scope,
            signer: self.signer,
            reservation: self.reservation,
            supply_obligation: self.supply_obligation,
            request_digest: self.request_digest,
            origin: self.origin,
            max_manifest_bytes: self.max_manifest_bytes,
            manifest_limits: self.manifest_limits,
            material_ready_by: self.schedule.material_ready_by(),
            filed_by: self.schedule.filed_by(),
            hard: self.schedule.clock.hard(),
        }
    }

    pub fn id(&self) -> CaseId {
        self.id
    }

    pub fn reservation(&self) -> Digest {
        self.reservation
    }

    pub fn check_delivery(
        &self,
        manifest: &CheckedManifestV1,
        original: &OriginalCommit,
        inclusion_daa: u64,
    ) -> Result<(), ContractError> {
        if manifest.original() != self.original {
            return Err(ContractError::Scope);
        }
        self.check_delivery_context(original, inclusion_daa)
    }

    fn check_delivery_context(&self, original: &OriginalCommit, inclusion_daa: u64) -> Result<(), ContractError> {
        if original.id() != self.original || original.manifest_material_id() != self.material {
            return Err(ContractError::Scope);
        }
        if !self.schedule.material_arrival_fits(self.origin.accepted_daa, inclusion_daa) {
            return Err(ContractError::Range);
        }
        Ok(())
    }

    pub fn unresolved_at(&self, inclusion_daa: u64) -> Result<MissingInspectionOutcome, ContractError> {
        self.schedule.missing_inspection_outcome(self.origin.accepted_daa, inclusion_daa)
    }
}

impl ManifestRequest {
    pub fn material_id(&self, original: &OriginalCommit) -> Result<MaterialId, ContractError> {
        if self.original != original.id() || self.manifest_root != original.manifest_root {
            return Err(ContractError::Scope);
        }
        Ok(original.manifest_material_id())
    }

    pub fn case_id(&self, original: &OriginalCommit, eligible_claimant: Claimant) -> Result<CaseId, ContractError> {
        if self.claimant != eligible_claimant {
            return Err(ContractError::Identity);
        }
        let material = self.material_id(original)?;
        Ok(manifest_case_id(self.original, eligible_claimant, material, self.inspection_scope))
    }

    pub fn signing_digest(&self) -> Digest {
        hash(b"misaka/palw/public-manifest-request-id/v1", self)
    }

    pub fn authenticate(
        &self,
        eligible_signer: PrincipalId,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<(), ContractError> {
        if self.signer != eligible_signer {
            return Err(ContractError::Identity);
        }
        authenticate_actor(eligible_signer, key, signature, self.signing_digest(), MANIFEST_REQUEST_SIGN_CONTEXT)
    }

    /// Authenticate a candidate against its expected case and original
    /// schedule before the native fold installs the case and debits its
    /// reservation atomically.
    pub fn verify_for_inclusion(
        &self,
        context: &ManifestRequestContext<'_>,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<PendingManifestCase, ContractError> {
        let id = self.check_inclusion(context)?;
        self.authenticate(context.signer, key, signature)?;
        Ok(PendingManifestCase {
            id,
            original: self.original,
            material: context.original.manifest_material_id(),
            claimant: context.claimant,
            inspection_scope: context.inspection_scope,
            signer: context.signer,
            reservation: context.reservation,
            supply_obligation: context.supply_obligation,
            request_digest: self.signing_digest(),
            origin: context.origin,
            max_manifest_bytes: context.max_manifest_bytes,
            manifest_limits: context.manifest_limits,
            schedule: context.schedule,
        })
    }

    fn check_inclusion(&self, context: &ManifestRequestContext<'_>) -> Result<CaseId, ContractError> {
        let id = self.case_id(context.original, context.claimant)?;
        if self.signer != context.signer || matches!(context.claimant, Claimant::Outsider(principal) if principal != context.signer) {
            return Err(ContractError::Identity);
        }
        if self.inspection_scope != context.inspection_scope || self.supply_obligation != context.supply_obligation {
            return Err(ContractError::Scope);
        }
        if self.reservation != context.reservation {
            return Err(ContractError::Budget);
        }
        let body_bytes = ManifestV1::fetch_size(context.max_manifest_bytes)?;
        if context.manifest_body_bytes_reserved < u64::from(body_bytes) {
            return Err(ContractError::Budget);
        }
        if !context.schedule.clock.accepts_first_notice(context.origin.accepted_daa)
            || self.collect_until != context.schedule.filed_by()
        {
            return Err(ContractError::Range);
        }
        Ok(id)
    }
}

fn authenticate_actor(
    actor: PrincipalId,
    key: &[u8; PUBKEY_BYTES],
    signature: &[u8; SIGNATURE_BYTES],
    id: Digest,
    context: &[u8],
) -> Result<(), ContractError> {
    if principal(key) != actor {
        return Err(ContractError::Identity);
    }
    if !verify_signature(key, id, signature, context) {
        return Err(ContractError::Signature);
    }
    Ok(())
}

/// Timing derived only from canonical original admission. On restore,
/// reconstruct it from that admission rather than trusting saved deadlines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OriginalClock {
    accepted_at: u64,
}

impl OriginalClock {
    pub fn new(accepted_at: u64) -> Result<Self, ContractError> {
        accepted_at.checked_add(10_001).ok_or(ContractError::Range)?;
        Ok(Self { accepted_at })
    }

    pub fn notice_until(self) -> u64 {
        self.accepted_at + 3_120
    }

    pub fn hard(self) -> u64 {
        self.accepted_at + 10_000
    }

    pub fn accepts_first_notice(self, now: u64) -> bool {
        (self.accepted_at..=self.notice_until()).contains(&now)
    }

    pub fn accepts_case_evidence(self, now: u64, opened_at: u64, collect_until: u64) -> bool {
        self.accepts_first_notice(opened_at) && collect_until <= self.hard() && (opened_at..=collect_until).contains(&now)
    }

    /// This predicate covers only time. Final also requires the native
    /// verification, unresolved-case and dependency checks.
    pub fn permits_final_time(self, now: u64) -> bool {
        now > self.notice_until() && now <= self.hard()
    }

    pub fn is_past_hard(self, now: u64) -> bool {
        now > self.hard()
    }
}

/// Bounds for the path from a fixed draw and available material to the first
/// canonical notice. Bounds must come from the selected profile's service
/// reservations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirstNoticeDemand {
    pub draw_ready_by: u64,
    pub material_ready_by: u64,
    pub cold_acquisition_daa: u64,
    pub full_inspection_daa: u64,
    pub notice_inclusion_daa: u64,
    pub missing_material_detected_by: u64,
    pub material_request_inclusion_daa: u64,
}

/// Worst-case path when a claimant opens a material case at the last legal
/// notice DAA. Filing includes evidence assembly and canonical inclusion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaterialCaseDemand {
    pub material_delivery_daa: u64,
    pub full_inspection_daa: u64,
    pub localization_daa: u64,
    pub filing_daa: u64,
    pub adjudication_and_recovery_daa: u64,
}

/// Timing contract only. The native admission path must also fund and book
/// both paths and preserve the per-claimant service and response capacity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerificationSchedule {
    clock: OriginalClock,
    first_notice_by: u64,
    first_material_request_by: u64,
    material_ready_by: u64,
    inspection_by: u64,
    localized_by: u64,
    filed_by: u64,
    terminal_by: u64,
}

/// Failure-side result while a case lacks objective inspection prerequisites.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissingInspectionOutcome {
    AwaitingMaterial,
    NoSlashHold,
    AbortedUnverified,
}

fn unresolved_inspection_at(now: u64, material_ready_by: u64, hard: u64) -> MissingInspectionOutcome {
    if now > hard {
        MissingInspectionOutcome::AbortedUnverified
    } else if now > material_ready_by {
        MissingInspectionOutcome::NoSlashHold
    } else {
        MissingInspectionOutcome::AwaitingMaterial
    }
}

impl VerificationSchedule {
    pub fn new(clock: OriginalClock, notice: FirstNoticeDemand, case: MaterialCaseDemand) -> Result<Self, ContractError> {
        if notice.draw_ready_by < clock.accepted_at
            || notice.material_ready_by < clock.accepted_at
            || notice.missing_material_detected_by < clock.accepted_at
        {
            return Err(ContractError::Range);
        }
        let first_notice_by = notice
            .draw_ready_by
            .max(notice.material_ready_by)
            .checked_add(notice.cold_acquisition_daa)
            .and_then(|daa| daa.checked_add(notice.full_inspection_daa))
            .and_then(|daa| daa.checked_add(notice.notice_inclusion_daa))
            .ok_or(ContractError::Range)?;
        let first_material_request_by = notice
            .draw_ready_by
            .max(notice.missing_material_detected_by)
            .checked_add(notice.material_request_inclusion_daa)
            .ok_or(ContractError::Range)?;
        let material_ready_by = clock.notice_until().checked_add(case.material_delivery_daa).ok_or(ContractError::Range)?;
        let inspection_by = material_ready_by.checked_add(case.full_inspection_daa).ok_or(ContractError::Range)?;
        let localized_by = inspection_by.checked_add(case.localization_daa).ok_or(ContractError::Range)?;
        let filed_by = localized_by.checked_add(case.filing_daa).ok_or(ContractError::Range)?;
        let terminal_by = filed_by.checked_add(case.adjudication_and_recovery_daa).ok_or(ContractError::Range)?;
        if first_notice_by.max(first_material_request_by) > clock.notice_until() || terminal_by > clock.hard() {
            return Err(ContractError::Budget);
        }
        Ok(Self {
            clock,
            first_notice_by,
            first_material_request_by,
            material_ready_by,
            inspection_by,
            localized_by,
            filed_by,
            terminal_by,
        })
    }

    pub fn first_notice_by(self) -> u64 {
        self.first_notice_by
    }

    pub fn first_material_request_by(self) -> u64 {
        self.first_material_request_by
    }

    pub fn material_ready_by(self) -> u64 {
        self.material_ready_by
    }

    pub fn inspection_by(self) -> u64 {
        self.inspection_by
    }

    pub fn localized_by(self) -> u64 {
        self.localized_by
    }

    pub fn filed_by(self) -> u64 {
        self.filed_by
    }

    pub fn terminal_by(self) -> u64 {
        self.terminal_by
    }

    pub fn hard(self) -> u64 {
        self.clock.hard()
    }

    /// Run only with the profile's authenticated availability observation.
    /// This checks time, leaving authentication and the inclusion opportunity
    /// to the native admission procedure.
    pub fn material_arrival_fits(self, opened_at: u64, ready_at: u64) -> bool {
        self.clock.accepts_first_notice(opened_at) && (opened_at..=self.material_ready_by).contains(&ready_at)
    }

    /// Apply after processing timely responses in the current canonical batch.
    /// The input is a still-unresolved case, regardless of partial deliveries
    /// or approval signatures. None of these outcomes removes its Q blocker.
    pub fn missing_inspection_outcome(self, opened_at: u64, now: u64) -> Result<MissingInspectionOutcome, ContractError> {
        if !self.clock.accepts_first_notice(opened_at) || now < opened_at {
            return Err(ContractError::Range);
        }
        Ok(unresolved_inspection_at(now, self.material_ready_by, self.clock.hard()))
    }
}

/// Pre-draw demand in a common physical resource pool. Reserve an entire
/// protected opportunity for every possible drawn bond. Outsider and
/// defensive demand are accounted for separately from that protected demand.
/// The native owner derives the projection and assigns it to distinct bonds
/// after the draw. This does not fund fees or allocate actual service slots.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProtectedRescueDemand {
    pub max_drawn_bonds: u64,
    pub per_drawn_bond: RemainingCapacity,
    pub max_outsiders: u64,
    pub per_outsider: RemainingCapacity,
    pub defensive: RemainingCapacity,
}

impl ProtectedRescueDemand {
    pub fn total(self) -> Result<RemainingCapacity, ContractError> {
        if self.max_drawn_bonds == 0 {
            return Err(ContractError::Range);
        }
        self.per_drawn_bond
            .checked_scale(self.max_drawn_bonds)?
            .checked_add(self.per_outsider.checked_scale(self.max_outsiders)?)?
            .checked_add(self.defensive)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_chunk_ranges_have_one_canonical_final_boundary() {
        let d = Digest([7; 64]);
        let entry = ManifestEntryV1 { subject: d, content_root: d, codec: d, total_bytes: 17, chunk_bytes: 8 };
        assert_eq!(entry.chunk_range(0), Ok(0..8));
        assert_eq!(entry.chunk_range(1), Ok(8..16));
        assert_eq!(entry.chunk_range(2), Ok(16..17));
        assert_eq!(entry.chunk_range(3), Err(ContractError::Range));
        assert_eq!(entry.chunk_range(u64::MAX), Err(ContractError::Range));
        assert_eq!(ManifestEntryV1 { chunk_bytes: 0, ..entry }.chunk_range(0), Err(ContractError::Range));
        assert_ne!(entry.chunk_material_id(d, 1), entry.chunk_material_id(d, 2));
        assert_ne!(entry.chunk_material_id(d, 2), entry.chunk_material_id(Digest([8; 64]), 2));
    }

    #[test]
    fn full_replay_approval_is_bound_to_one_drawn_bond_and_the_full_scope() {
        let d = Digest([7; 64]);
        let bond = PanelBond { transaction: d, index: 0 };
        let context = ReplayApprovalContext { original: OriginalId(d), draw: d, bond, member: PrincipalId(d), full_scope: d };
        let approval = FullReplayApproval {
            original: context.original,
            draw: context.draw,
            bond,
            member: context.member,
            full_scope: context.full_scope,
        };
        assert_eq!(approval.check_subject(&context), Ok(()));

        let another_bond = FullReplayApproval { bond: PanelBond { index: 1, ..bond }, ..approval.clone() };
        assert_eq!(another_bond.check_subject(&context), Err(ContractError::Identity));
        assert_ne!(another_bond.signing_digest(), approval.signing_digest());
        let another_draw = FullReplayApproval { draw: Digest([8; 64]), ..approval.clone() };
        assert_eq!(another_draw.check_subject(&context), Err(ContractError::Scope));
        let partial_scope = FullReplayApproval { full_scope: Digest([9; 64]), ..approval };
        assert_eq!(partial_scope.check_subject(&context), Err(ContractError::Scope));
    }

    #[test]
    fn rescue_demand_includes_every_drawn_bond_and_independent_response_capacity() {
        let demand = ProtectedRescueDemand {
            max_drawn_bonds: 3,
            per_drawn_bond: RemainingCapacity { transmitted_bytes: 100, additional_work: 40, retained_bytes: 20 },
            max_outsiders: 2,
            per_outsider: RemainingCapacity { transmitted_bytes: 50, additional_work: 15, retained_bytes: 5 },
            defensive: RemainingCapacity { transmitted_bytes: 25, additional_work: 10, retained_bytes: 15 },
        };
        assert_eq!(demand.total(), Ok(RemainingCapacity { transmitted_bytes: 425, additional_work: 160, retained_bytes: 85 }));
        assert_eq!(ProtectedRescueDemand { max_drawn_bonds: 0, ..demand }.total(), Err(ContractError::Range));
        assert_eq!(ProtectedRescueDemand { max_drawn_bonds: u64::MAX, ..demand }.total(), Err(ContractError::Budget));
        assert_eq!(
            ProtectedRescueDemand { defensive: RemainingCapacity { retained_bytes: u64::MAX, ..demand.defensive }, ..demand }.total(),
            Err(ContractError::Budget)
        );
    }

    #[test]
    fn verification_schedule_reserves_both_windows_from_original_admission() {
        let clock = OriginalClock::new(100).unwrap();
        let notice = FirstNoticeDemand {
            draw_ready_by: 120,
            material_ready_by: 200,
            cold_acquisition_daa: 1_000,
            full_inspection_daa: 1_000,
            notice_inclusion_daa: 1_020,
            missing_material_detected_by: 3_200,
            material_request_inclusion_daa: 20,
        };
        let case = MaterialCaseDemand {
            material_delivery_daa: 3_000,
            full_inspection_daa: 1_000,
            localization_daa: 1_000,
            filing_daa: 1_000,
            adjudication_and_recovery_daa: 880,
        };
        let schedule = VerificationSchedule::new(clock, notice, case).unwrap();
        assert_eq!(schedule.first_notice_by(), clock.notice_until());
        assert_eq!(schedule.first_material_request_by(), clock.notice_until());
        assert_eq!(schedule.material_ready_by(), 6_220);
        assert_eq!(schedule.inspection_by(), 7_220);
        assert_eq!(schedule.localized_by(), 8_220);
        assert_eq!(schedule.filed_by(), 9_220);
        assert_eq!(schedule.terminal_by(), clock.hard());
        assert!(schedule.material_arrival_fits(clock.notice_until(), schedule.material_ready_by()));
        assert!(!schedule.material_arrival_fits(clock.notice_until(), schedule.material_ready_by() + 1));
        assert!(!schedule.material_arrival_fits(clock.notice_until() + 1, schedule.material_ready_by()));
        let opened = clock.notice_until();
        assert_eq!(schedule.missing_inspection_outcome(opened, opened - 1), Err(ContractError::Range));
        assert_eq!(
            schedule.missing_inspection_outcome(opened, schedule.material_ready_by()),
            Ok(MissingInspectionOutcome::AwaitingMaterial)
        );
        assert_eq!(schedule.missing_inspection_outcome(opened, clock.hard()), Ok(MissingInspectionOutcome::NoSlashHold));
        assert_eq!(schedule.missing_inspection_outcome(opened, clock.hard() + 1), Ok(MissingInspectionOutcome::AbortedUnverified));
        assert_eq!(
            VerificationSchedule::new(clock, FirstNoticeDemand { material_ready_by: 201, ..notice }, case),
            Err(ContractError::Budget)
        );
        assert_eq!(
            VerificationSchedule::new(clock, FirstNoticeDemand { material_request_inclusion_daa: 21, ..notice }, case),
            Err(ContractError::Budget)
        );
        assert_eq!(
            VerificationSchedule::new(clock, notice, MaterialCaseDemand { filing_daa: 1_001, ..case }),
            Err(ContractError::Budget)
        );
        assert_eq!(
            VerificationSchedule::new(clock, notice, MaterialCaseDemand { filing_daa: u64::MAX, ..case }),
            Err(ContractError::Range)
        );
    }

    #[test]
    fn manifest_bytes_are_shared_but_each_claimant_keeps_a_case() {
        let d = Digest([7; 64]);
        let original = OriginalCommit {
            scope: ChainScope { network: d, ruleset: d, branch: d },
            job: d,
            execution: d,
            miner: PrincipalId(d),
            registered_class: d,
            artifact: d,
            program: d,
            prompt: d,
            input_tokens: d,
            tokenizer: d,
            execution_rules: d,
            generation_seed: [0; 32],
            initial_state: d,
            dependencies: d,
            prefill: 1,
            decode: 1,
            max_context: 1,
            output: d,
            trace_root: d,
            manifest_root: manifest_root(d, b"manifest", 8).unwrap(),
            service_contract: d,
        };
        let first = Claimant::DrawnBond(PanelBond { transaction: d, index: 0 });
        let second = Claimant::DrawnBond(PanelBond { transaction: d, index: 1 });
        let request = ManifestRequest {
            original: original.id(),
            manifest_root: original.manifest_root,
            claimant: first,
            signer: PrincipalId(d),
            inspection_scope: d,
            supply_obligation: d,
            collect_until: 100,
            reservation: d,
        };
        let other = ManifestRequest { claimant: second, ..request.clone() };
        let material = original.authenticate_manifest(b"manifest", 8).unwrap();
        assert_eq!(material.bytes(), b"manifest");
        assert_eq!(request.material_id(&original), Ok(material.id()));
        assert!(matches!(original.authenticate_manifest(b"manifest", 7), Err(ContractError::Range)));
        assert!(matches!(original.authenticate_manifest(b"Manifest", 8), Err(ContractError::Identity)));
        assert_ne!(manifest_root(Digest([8; 64]), b"manifest", 8), Ok(original.manifest_root));
        assert_eq!(request.material_id(&original), other.material_id(&original));
        assert_ne!(request.case_id(&original, first), other.case_id(&original, second));
        assert_eq!(other.case_id(&original, first), Err(ContractError::Identity));

        let retry = ManifestRequest { collect_until: 101, reservation: Digest([8; 64]), ..request.clone() };
        assert_eq!(request.case_id(&original, first), retry.case_id(&original, first));
        assert_ne!(request.signing_digest(), retry.signing_digest());

        let wrong_root = ManifestRequest { manifest_root: Digest([8; 64]), ..request.clone() };
        assert_eq!(wrong_root.material_id(&original), Err(ContractError::Scope));

        let clock = OriginalClock::new(100).unwrap();
        let schedule = VerificationSchedule {
            clock,
            first_notice_by: 3_220,
            first_material_request_by: 3_220,
            material_ready_by: 6_220,
            inspection_by: 7_220,
            localized_by: 8_220,
            filed_by: 9_220,
            terminal_by: 10_100,
        };
        let mut context = ManifestRequestContext {
            original: &original,
            claimant: first,
            signer: PrincipalId(d),
            inspection_scope: d,
            supply_obligation: d,
            reservation: d,
            max_manifest_bytes: ManifestV1::WIRE_BYTES,
            manifest_limits: d,
            manifest_body_bytes_reserved: u64::from(ManifestV1::WIRE_BYTES),
            schedule,
            origin: AcceptedOrigin {
                accepting_block: crate::rows::NativeHash64([1; 64]),
                carrier_block: crate::rows::NativeHash64([2; 64]),
                carrier_transaction: crate::rows::NativeHash64([3; 64]),
                accepted_tx_ordinal: 1,
                event_ordinal: 0,
                accepted_daa: clock.notice_until(),
                accepted_blue_score: clock.notice_until(),
            },
        };
        let request = ManifestRequest { collect_until: schedule.filed_by(), ..request };
        assert_eq!(request.check_inclusion(&context), request.case_id(&original, first));
        context.manifest_body_bytes_reserved -= 1;
        assert_eq!(request.check_inclusion(&context), Err(ContractError::Budget));
        context.manifest_body_bytes_reserved += 1;
        context.max_manifest_bytes -= 1;
        assert_eq!(request.check_inclusion(&context), Err(ContractError::Range));
        context.max_manifest_bytes += 1;
        assert_eq!(wrong_root.check_inclusion(&context), Err(ContractError::Scope));
        let other_reservation = ManifestRequest { reservation: Digest([8; 64]), ..request.clone() };
        assert_eq!(other_reservation.check_inclusion(&context), Err(ContractError::Budget));
        let shortened = ManifestRequest { collect_until: schedule.filed_by() - 1, ..request.clone() };
        assert_eq!(shortened.check_inclusion(&context), Err(ContractError::Range));
        context.origin.accepted_daa += 1;
        assert_eq!(request.check_inclusion(&context), Err(ContractError::Range));

        let mut rules = [ManifestScopeRules { codec: d, max_bytes: 16, chunk_bytes: 8 }; MaterialScope::COUNT];
        let mut manifest = ManifestV1 {
            service_contract: original.service_contract,
            entries: [ManifestEntryV1 { subject: d, content_root: d, codec: d, total_bytes: 9, chunk_bytes: 8 }; MaterialScope::COUNT],
        };
        let program_codec = tir_program_material_codec_v1();
        rules[MaterialScope::Program as usize] = ManifestScopeRules { codec: program_codec, max_bytes: 16, chunk_bytes: 16 };
        manifest.entries[MaterialScope::Program as usize].codec = program_codec;
        manifest.entries[MaterialScope::Program as usize].chunk_bytes = 16;
        let max = u32::try_from(borsh::object_length(&manifest).unwrap()).unwrap();
        assert_eq!(ManifestV1::fetch_size(max), Ok(max));
        assert_eq!(ManifestV1::fetch_size(max - 1), Err(ContractError::Range));
        let mut bound_original = original.clone();
        let prompt = "abcあdef";
        let prompt_rules = ManifestScopeRules { codec: prompt_text_material_codec_v1(), max_bytes: 16, chunk_bytes: 16 };
        let prompt_entry = prompt_text_material_v1(d, prompt, prompt_rules).unwrap();
        assert_eq!(prompt_entry.total_bytes, 9);
        rules[MaterialScope::Prompt as usize] = prompt_rules;
        manifest.entries[MaterialScope::Prompt as usize] = prompt_entry;
        bound_original.prompt = prompt_entry.subject;
        assert_eq!(bound_original.bind_manifest(&manifest, &rules, 89, max), Err(ContractError::Budget));
        assert_eq!(bound_original.bind_manifest(&manifest, &rules, 90, max - 1), Err(ContractError::Range));
        assert_eq!(bound_original.manifest_root, original.manifest_root);
        let bytes = bound_original.bind_manifest(&manifest, &rules, 90, max).unwrap();
        let authenticated = bound_original.authenticate_manifest(&bytes, max).unwrap();
        let checked = authenticated.decode(&bound_original, &rules, 90).unwrap();
        assert_eq!(checked.open_prompt_text(prompt.as_bytes()), Ok(prompt));
        assert_eq!(checked.open_prompt_text(b"abcXXXdef"), Err(ContractError::Identity));
        assert_eq!(checked.open_prompt_text(&[255; 9]), Err(ContractError::NonCanonical));
        assert_eq!(checked.original(), bound_original.id());
        let program = checked.chunk(MaterialScope::Program, 0).unwrap();
        assert_eq!(program.codec, program_codec);
        assert_eq!(program.range, 0..9);
        assert_eq!(checked.chunk(MaterialScope::Program, 1), Err(ContractError::Range));
        let chunk = checked.chunk(MaterialScope::Trace, 1).unwrap();
        assert_eq!(chunk.range, 8..9);
        assert_eq!(chunk.content_root, d);
        assert_eq!(chunk.codec, d);
        assert_eq!(chunk.id, manifest.entry(MaterialScope::Trace).chunk_material_id(d, 1).unwrap());
        assert_eq!(checked.chunk(MaterialScope::Trace, 2), Err(ContractError::Range));
        assert_eq!(authenticated.decode(&bound_original, &rules, 89), Err(ContractError::Budget));
        assert_eq!(authenticated.decode(&original, &rules, 90), Err(ContractError::Scope));
        let pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([41; 32]);
        let key = pair.verification_key.as_ref();
        let request = ManifestRequest {
            original: bound_original.id(),
            manifest_root: bound_original.manifest_root,
            signer: principal(key),
            ..request
        };
        let limits_id = manifest_limits_id_v1(&rules, 90, max);
        let context = ManifestRequestContext {
            original: &bound_original,
            manifest_limits: limits_id,
            signer: request.signer,
            origin: AcceptedOrigin { accepted_daa: clock.notice_until(), ..context.origin },
            ..context
        };
        let signature =
            libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &request.signing_digest().0, MANIFEST_REQUEST_SIGN_CONTEXT, [0; 32])
                .unwrap();
        let carrier = SignedManifestRequestV1 { request: request.clone(), key: *key, signature: *signature.as_ref() };
        let mut wire = borsh::to_vec(&carrier).unwrap();
        assert_eq!(wire.len(), SignedManifestRequestV1::MAX_WIRE_BYTES);
        assert_eq!(SignedManifestRequestV1::decode(&wire), Ok(carrier.clone()));
        wire.push(0);
        assert_eq!(SignedManifestRequestV1::decode(&wire), Err(ContractError::NonCanonical));
        let outsider = SignedManifestRequestV1 {
            request: ManifestRequest { claimant: Claimant::Outsider(request.signer), ..request.clone() },
            ..carrier
        };
        let wire = borsh::to_vec(&outsider).unwrap();
        assert_eq!(wire.len() + 4, SignedManifestRequestV1::MAX_WIRE_BYTES);
        assert_eq!(SignedManifestRequestV1::decode(&wire), Ok(outsider));
        let pending = request.verify_for_inclusion(&context, key, signature.as_ref()).unwrap();
        assert_eq!(pending.id(), request.case_id(&bound_original, first).unwrap());
        assert_eq!(pending.reservation(), context.reservation);
        let stored = borsh::to_vec(&pending.record()).unwrap();
        assert_eq!(stored.len(), PendingManifestCaseRecordV1::WIRE_BYTES);
        let case_key = PendingManifestCaseRecordV1::key(bound_original.id(), pending.id());
        let restored = PendingManifestCaseRecordV1::decode_row(&case_key, &stored).unwrap();
        assert_eq!(restored.signer(), request.signer);
        let changed_scope = PendingManifestCaseRecordV1 { inspection_scope: Digest([99; 64]), ..restored };
        assert_eq!(
            PendingManifestCaseRecordV1::decode_row(&case_key, &borsh::to_vec(&changed_scope).unwrap()),
            Err(ContractError::Identity)
        );
        let other_claimant = PendingManifestCaseRecordV1 { claimant: ManifestClaimantRecordV1::new(second), ..restored };
        assert_eq!(
            PendingManifestCaseRecordV1::decode_row(&case_key, &borsh::to_vec(&other_claimant).unwrap()),
            Err(ContractError::Identity)
        );
        let wrong_original = PendingManifestCaseRecordV1::key(OriginalId(Digest([0; 64])), pending.id());
        assert_eq!(PendingManifestCaseRecordV1::decode_row(&wrong_original, &stored), Err(ContractError::Identity));
        assert_eq!(PendingManifestCaseRecordV1::decode_row(&[], &stored), Err(ContractError::Identity));
        assert_eq!(
            restored.check_admission(100, clock.notice_until(), max, limits_id, schedule.material_ready_by(), schedule.filed_by()),
            Ok(())
        );
        assert_eq!(
            restored.check_admission(100, clock.notice_until() - 1, max, limits_id, schedule.material_ready_by(), schedule.filed_by()),
            Err(ContractError::Scope)
        );
        assert_eq!(restored.check_restore(&pending), Ok(()));
        assert_eq!(restored.unresolved_at(schedule.material_ready_by()), Ok(MissingInspectionOutcome::AwaitingMaterial));
        assert_eq!(restored.unresolved_at(clock.hard()), Ok(MissingInspectionOutcome::NoSlashHold));
        assert_eq!(restored.unresolved_at(clock.hard() + 1), Ok(MissingInspectionOutcome::AbortedUnverified));
        let alternate = AcceptedOrigin { accepted_tx_ordinal: restored.origin.accepted_tx_ordinal + 1, ..restored.origin };
        let altered = PendingManifestCaseRecordV1 { origin: alternate, ..restored };
        assert_eq!(altered.check_restore(&pending), Err(ContractError::Scope));
        let altered = PendingManifestCaseRecordV1 { max_manifest_bytes: restored.max_manifest_bytes + 1, ..restored };
        assert_eq!(altered.check_restore(&pending), Err(ContractError::Scope));
        assert_eq!(pending.check_delivery(&checked, &bound_original, schedule.material_ready_by()), Ok(()));
        let mut changed_bytes = bytes.clone();
        changed_bytes[0] ^= 1;
        let response = SignedManifestResponseV1 {
            case: pending.id(),
            supply_obligation: context.supply_obligation,
            provider: principal(key),
            body_root: bound_original.manifest_root,
        };
        let provider_signature =
            libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &response.signing_digest().0, MANIFEST_RESPONSE_SIGN_CONTEXT, [0; 32])
                .unwrap();
        let header = borsh::to_vec(&ManifestResponseCarrierV1 {
            response: response.clone(),
            key: *key,
            signature: *provider_signature.as_ref(),
        })
        .unwrap();
        assert_eq!(header.len(), ManifestResponseCarrierV1::HEADER_BYTES);
        let delivery_origin = AcceptedOrigin {
            accepting_block: crate::rows::NativeHash64([4; 64]),
            carrier_block: crate::rows::NativeHash64([5; 64]),
            carrier_transaction: crate::rows::NativeHash64([6; 64]),
            accepted_daa: schedule.material_ready_by(),
            accepted_blue_score: schedule.material_ready_by(),
            ..context.origin
        };
        let verify = |origin, body: &[u8], total_limit| {
            let mut packet = header.clone();
            packet.extend_from_slice(body);
            pending.verify_provider_response_bytes(&packet, principal(key), &bound_original, origin, &rules, total_limit)
        };
        assert_eq!(
            pending.verify_provider_response_bytes(
                &header[..header.len() - 1],
                principal(key),
                &bound_original,
                delivery_origin,
                &rules,
                90,
            ),
            Err(ContractError::NonCanonical)
        );
        assert_eq!(verify(delivery_origin, &vec![0; max as usize + 1], 90), Err(ContractError::Range));
        assert_eq!(verify(delivery_origin, &bytes, 89), Err(ContractError::Scope));
        assert_eq!(verify(delivery_origin, &changed_bytes, 90), Err(ContractError::Identity));
        let wrong_response = SignedManifestResponseV1 {
            body_root: manifest_root(bound_original.scope.ruleset, &changed_bytes, max).unwrap(),
            ..response.clone()
        };
        let wrong_signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &pair.signing_key,
            &wrong_response.signing_digest().0,
            MANIFEST_RESPONSE_SIGN_CONTEXT,
            [0; 32],
        )
        .unwrap();
        let wrong = pending
            .verify_provider_response(
                &wrong_response,
                principal(key),
                &bound_original,
                delivery_origin,
                &changed_bytes,
                &rules,
                90,
                key,
                wrong_signature.as_ref(),
            )
            .unwrap();
        let verified = verify(delivery_origin, &bytes, 90).unwrap();
        let stored = borsh::to_vec(&verified.record()).unwrap();
        let restored = ManifestResponseRecordV1::decode_row(&verified.record().key(), &stored).unwrap();
        assert_eq!(
            ManifestResponseRecordV1::decode_row(&[0; ManifestResponseRecordV1::KEY_BYTES], &stored),
            Err(ContractError::Identity)
        );
        assert_eq!(restored.check_case(&pending.record()), Ok(()));
        let wrong_case = PendingManifestCaseRecordV1 { id: CaseId(Digest([8; 64])), ..pending.record() };
        assert_eq!(restored.check_case(&wrong_case), Err(ContractError::Scope));
        let wrong_obligation = PendingManifestCaseRecordV1 { supply_obligation: Digest([99; 64]), ..pending.record() };
        assert_eq!(restored.check_case(&wrong_obligation), Err(ContractError::Scope));
        assert_eq!(restored.check_restore(&verified), Ok(()));
        let mut packet = header.clone();
        packet.extend_from_slice(&bytes);
        let restore = |origin, provider| {
            pending.restore_provider_response_bytes(&restored, &packet, provider, &bound_original, origin, &rules, 90)
        };
        assert_eq!(restore(delivery_origin, principal(key)).as_ref(), Ok(&verified));
        assert_eq!(restore(delivery_origin, PrincipalId(Digest([99; 64]))), Err(ContractError::Identity));
        assert_eq!(stored.len(), ManifestResponseRecordV1::WIRE_BYTES);
        let alternate = AcceptedOrigin { carrier_transaction: crate::rows::NativeHash64([7; 64]), ..delivery_origin };
        assert_eq!(restore(alternate, principal(key)), Err(ContractError::Scope));
        assert_eq!(restored.check_restore(&verify(alternate, &bytes, 90).unwrap()), Err(ContractError::Scope));
        assert_eq!(verify(context.origin, &bytes, 90), Err(ContractError::Scope));
        assert_eq!(verify(delivery_origin, &bytes, 91), Err(ContractError::Scope));
        assert_eq!(restored.check_restore(&wrong), Err(ContractError::Scope));
        assert_eq!(wrong.into_outcome(), ManifestDeliveryV1::WrongMaterial);
        assert_eq!(pending.check_delivery(&checked, &bound_original, schedule.material_ready_by() + 1), Err(ContractError::Range));
        assert_eq!(verified.into_outcome(), ManifestDeliveryV1::Checked(checked));
        assert_eq!(pending.unresolved_at(clock.hard() + 1), Ok(MissingInspectionOutcome::AbortedUnverified));
        let changed = OriginalCommit { trace_root: Digest([8; 64]), ..bound_original };
        assert_eq!(authenticated.decode(&changed, &rules, 90), Err(ContractError::Scope));
    }

    #[test]
    fn original_clock_keeps_inclusive_notice_and_hard_boundaries() {
        let clock = OriginalClock::new(100).unwrap();
        let notice = 3_220;
        let hard = 10_100;
        assert!(!clock.accepts_first_notice(99));
        assert!(clock.accepts_first_notice(100));
        assert!(clock.accepts_first_notice(notice));
        assert!(!clock.accepts_first_notice(notice + 1));
        assert!(!clock.permits_final_time(notice));
        assert!(clock.permits_final_time(notice + 1));
        assert!(clock.permits_final_time(hard));
        assert!(!clock.permits_final_time(hard + 1));
        assert!(clock.accepts_case_evidence(hard, notice, hard));
        assert!(!clock.accepts_case_evidence(hard, notice, hard - 1));
        assert!(!clock.accepts_case_evidence(hard, notice + 1, hard));
        assert!(!clock.accepts_case_evidence(hard, notice, hard + 1));
        assert!(!clock.is_past_hard(hard));
        assert!(clock.is_past_hard(hard + 1));
        assert_eq!(OriginalClock::new(u64::MAX - 10_000), Err(ContractError::Range));
    }
}
