//! Shared contracts for the public-materials profile. These values carry no
//! native admission, availability, verdict, or settlement authority.

use crate::ContractError;
use crate::identity::{ChainScope, Digest, PUBKEY_BYTES, PrincipalId, SIGNATURE_BYTES, hash, principal, verify_signature};
use crate::reservations::RemainingCapacity;
use borsh::{BorshDeserialize, BorshSerialize};

pub const ORIGINAL_SIGN_CONTEXT: &[u8] = b"misaka/palw/public-original/v1";
pub const FULL_REPLAY_SIGN_CONTEXT: &[u8] = b"misaka/palw/public-full-replay/v1";
pub const MANIFEST_REQUEST_SIGN_CONTEXT: &[u8] = b"misaka/palw/public-manifest-request/v1";
pub const MANIFEST_RESPONSE_SIGN_CONTEXT: &[u8] = b"misaka/palw/public-manifest-response/v1";

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

/// The canonical identity native admission must resolve from the draw or
/// from admitted outsider eligibility before using a request's case ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Claimant {
    DrawnBond(PanelBond),
    Outsider(PrincipalId),
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

    fn manifest_material_id(&self) -> MaterialId {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    /// Body bytes still booked for this claimant and obligation. The native
    /// consumer accounts for the envelope, verification work and retention separately.
    pub manifest_body_bytes_reserved: u64,
    pub schedule: VerificationSchedule,
    pub inclusion_daa: u64,
}

/// An authenticated request ready for the native fold to install alongside
/// its reservation. Delivery of this one object does not close inspection.
#[derive(Debug, PartialEq, Eq)]
pub struct PendingManifestCase {
    id: CaseId,
    original: OriginalId,
    material: MaterialId,
    reservation: Digest,
    supply_obligation: Digest,
    request_digest: Digest,
    opened_at: u64,
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

/// Durable identity for a pending case. Restore must reauthenticate the
/// signed request against its original canonical reservation and compare.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PendingManifestCaseRecordV1 {
    id: CaseId,
    original: OriginalId,
    material: MaterialId,
    reservation: Digest,
    request_digest: Digest,
    opened_at: u64,
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
}

impl PendingManifestCaseRecordV1 {
    pub fn check_restore(&self, pending: &PendingManifestCase) -> Result<(), ContractError> {
        if *self != pending.record() {
            return Err(ContractError::Scope);
        }
        Ok(())
    }
}

impl PendingManifestCase {
    /// Validate the provider named by the actual native obligation.
    /// Neither the signature nor this response settles the case.
    #[allow(clippy::too_many_arguments)]
    pub fn verify_provider_response(
        &self,
        response: &SignedManifestResponseV1,
        expected_provider: PrincipalId,
        original: &OriginalCommit,
        inclusion_daa: u64,
        bytes: &[u8],
        scope_limits: &[ManifestScopeRules; MaterialScope::COUNT],
        total_limit: u64,
        max_manifest_bytes: u32,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<ManifestDeliveryV1, ContractError> {
        self.check_delivery_context(original, inclusion_daa)?;
        if response.case != self.id || response.supply_obligation != self.supply_obligation {
            return Err(ContractError::Scope);
        }
        if response.provider != expected_provider {
            return Err(ContractError::Identity);
        }
        let body_root = manifest_root(original.scope.ruleset, bytes, max_manifest_bytes)?;
        if body_root != response.body_root {
            return Err(ContractError::Identity);
        }
        authenticate_actor(expected_provider, key, signature, response.signing_digest(), MANIFEST_RESPONSE_SIGN_CONTEXT)?;
        if body_root != original.manifest_root {
            return Err(ContractError::Identity);
        }
        let authenticated = AuthenticatedManifest { id: self.material, bytes };
        Ok(match authenticated.decode(original, scope_limits, total_limit) {
            Ok(manifest) => ManifestDeliveryV1::Checked(manifest),
            Err(error) => ManifestDeliveryV1::CommittedButInvalid(error),
        })
    }

    pub fn record(&self) -> PendingManifestCaseRecordV1 {
        PendingManifestCaseRecordV1 {
            id: self.id,
            original: self.original,
            material: self.material,
            reservation: self.reservation,
            request_digest: self.request_digest,
            opened_at: self.opened_at,
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
        if !self.schedule.material_arrival_fits(self.opened_at, inclusion_daa) {
            return Err(ContractError::Range);
        }
        Ok(())
    }

    pub fn unresolved_at(&self, inclusion_daa: u64) -> Result<MissingInspectionOutcome, ContractError> {
        self.schedule.missing_inspection_outcome(self.opened_at, inclusion_daa)
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
        Ok(CaseId(hash(b"misaka/palw/public-manifest-case/v1", &(self.original, eligible_claimant, material, self.inspection_scope))))
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
            reservation: context.reservation,
            supply_obligation: context.supply_obligation,
            request_digest: self.signing_digest(),
            opened_at: context.inclusion_daa,
            schedule: context.schedule,
        })
    }

    fn check_inclusion(&self, context: &ManifestRequestContext<'_>) -> Result<CaseId, ContractError> {
        let id = self.case_id(context.original, context.claimant)?;
        if self.signer != context.signer {
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
        if !context.schedule.clock.accepts_first_notice(context.inclusion_daa) || self.collect_until != context.schedule.filed_by() {
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
        Ok(if self.clock.is_past_hard(now) {
            MissingInspectionOutcome::AbortedUnverified
        } else if now > self.material_ready_by {
            MissingInspectionOutcome::NoSlashHold
        } else {
            MissingInspectionOutcome::AwaitingMaterial
        })
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
            manifest_body_bytes_reserved: u64::from(ManifestV1::WIRE_BYTES),
            schedule,
            inclusion_daa: clock.notice_until(),
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
        context.inclusion_daa += 1;
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
        assert_eq!(bound_original.bind_manifest(&manifest, &rules, 89, max), Err(ContractError::Budget));
        assert_eq!(bound_original.bind_manifest(&manifest, &rules, 90, max - 1), Err(ContractError::Range));
        assert_eq!(bound_original.manifest_root, original.manifest_root);
        let bytes = bound_original.bind_manifest(&manifest, &rules, 90, max).unwrap();
        let authenticated = bound_original.authenticate_manifest(&bytes, max).unwrap();
        let checked = authenticated.decode(&bound_original, &rules, 90).unwrap();
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
        let context = ManifestRequestContext {
            original: &bound_original,
            signer: request.signer,
            inclusion_daa: clock.notice_until(),
            ..context
        };
        let signature =
            libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &request.signing_digest().0, MANIFEST_REQUEST_SIGN_CONTEXT, [0; 32])
                .unwrap();
        let pending = request.verify_for_inclusion(&context, key, signature.as_ref()).unwrap();
        assert_eq!(pending.id(), request.case_id(&bound_original, first).unwrap());
        assert_eq!(pending.reservation(), context.reservation);
        let stored = borsh::to_vec(&pending.record()).unwrap();
        let restored: PendingManifestCaseRecordV1 = borsh::from_slice(&stored).unwrap();
        assert_eq!(restored.check_restore(&pending), Ok(()));
        let altered = PendingManifestCaseRecordV1 { opened_at: restored.opened_at - 1, ..restored };
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
        let verify = |body: &[u8], total_limit| {
            pending.verify_provider_response(
                &response,
                principal(key),
                &bound_original,
                schedule.material_ready_by(),
                body,
                &rules,
                total_limit,
                max,
                key,
                provider_signature.as_ref(),
            )
        };
        assert_eq!(verify(&bytes, 89), Ok(ManifestDeliveryV1::CommittedButInvalid(ContractError::Budget)));
        assert_eq!(verify(&changed_bytes, 90), Err(ContractError::Identity));
        assert_eq!(pending.check_delivery(&checked, &bound_original, schedule.material_ready_by() + 1), Err(ContractError::Range));
        assert_eq!(verify(&bytes, 90), Ok(ManifestDeliveryV1::Checked(checked)));
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
