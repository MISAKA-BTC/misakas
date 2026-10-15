//! Independent readonly Job/Final audit factory for owning39
//! (`CheckedNativeJobAuditV39`), and the complete Master Final audit built on
//! it. Every value is re-derived from the fully authenticated original aux
//! stream of ONE independently anchored state under the auditor's own Params:
//! fixed header, book, producer consent, class (from live rows or an archived
//! body authenticated by its immutable header), requested maximum, quota plan,
//! complete maximum contract and temporal plan, actual Commit, settlement,
//! spent nonce, funding receipt, single credit and Master binding.
//!
//! A known missing record is a named failure, never success. This view is a
//! data-identity and monetary-conservation audit: it is not a proof that the
//! LLM computation is correct (PRINCIPLES §8), grants no Final, no mining
//! receipt and no consensus weight, and does not replace sampled-validator
//! approval or the UTXO redemption checks.
use super::*;
use crate::{
    palw_native_audit_proof_v39::{NativeAuditRowReaderV39, VerifiedNativeAuxRowsV39},
    palw_native_job_audit_v39::CheckedNativeJobAuditV39,
    palw_native_master_v39::{CheckedNativeMasterFinalAuditV39, NativeMasterFundedComponentV39, derive_native_master_final_audit_v39},
    palw_native_state_anchor_v39::CheckedNativeStateAnchorV39,
};

/// Restoration budget for one job audit: the complete source, consent and
/// post, plus bounded fixed rows.
const JOB_AUDIT_RESTORE_MAX: usize = misaka_palw_kernel::flat_source39::SOURCE39_MAX_BYTES
    + NativeConsentWireV39::MAX_BYTES
    + NativeCommitWireV39::MAX_BYTES
    + (1 << 20);

struct Rows<'a, R> {
    rows: &'a VerifiedNativeAuxRowsV39<R>,
    remaining: usize,
}
impl<R: NativeAuditRowReaderV39> Rows<'_, R> {
    fn get(&mut self, table: u8, key: Vec<u8>, cap: usize) -> Result<Option<Vec<u8>>, String> {
        let Some(raw) = self.rows.row(table, &key, cap.min(self.remaining))? else { return Ok(None) };
        self.remaining = self.remaining.checked_sub(raw.len()).ok_or("audit39 job restoration budget exhausted")?;
        Ok(Some(raw))
    }
    fn need(&mut self, table: u8, key: Vec<u8>, cap: usize, what: &str) -> Result<Vec<u8>, String> {
        self.get(table, key, cap)?.ok_or_else(|| format!("audit39 original {what} row absent"))
    }
}
fn decode<T: BorshDeserialize>(raw: &[u8], what: &str) -> Result<T, String> {
    borsh::from_slice(raw).map_err(|_| format!("audit39 original {what} grammar"))
}

/// The auditor's own owning policies at the anchored DAA (no caller policy).
pub(super) fn audit_policies_v39(
    parameters: &crate::config::params::Params,
    daa: u64,
) -> Result<NativeExecutionPoliciesV39, String> {
    let p = &parameters
        .palw_native_execution_v39
        .as_ref()
        .filter(|f| f.activation.is_active(daa))
        .ok_or("audit39 owning fence inactive at the anchored state")?
        .policies;
    let final_policy = &parameters
        .palw_native_final_v27
        .as_ref()
        .filter(|f| f.activation.is_active(daa))
        .ok_or("audit39 original Final fence inactive")?
        .policy;
    let funded = &parameters
        .palw_funded_execution_v11
        .as_ref()
        .filter(|f| f.activation.is_active(daa))
        .ok_or("audit39 original shared execution fence inactive")?
        .policies;
    p.check(final_policy.network_domain)?;
    if p.final_policy != final_policy.root() || &p.execution != funded {
        return Err("audit39 own owning/Final/shared policy relation differs".into());
    }
    Ok(p.clone())
}

impl PalwChainStateV2 {
    /// Derive one checked job audit from the complete authenticated stream.
    /// `archive` supplies only bodies that collection53 or class/source
    /// retention release removed; each is authenticated against the original
    /// immutable header in this same anchored state.
    pub fn native_job_audit_from_stream_v39<R: NativeAuditRowReaderV39>(
        rows: &VerifiedNativeAuxRowsV39<R>,
        anchor: &CheckedNativeStateAnchorV39,
        parameters: &crate::config::params::Params,
        job: Digest,
        archive: Option<&dyn NativeOriginalSourceArchiveV39>,
    ) -> Result<CheckedNativeJobAuditV39, String> {
        let daa = anchor.daa();
        if rows.state_root() != anchor.inner() {
            return Err("audit39 job stream and independent anchor differ".into());
        }
        let p = audit_policies_v39(parameters, daa)?;
        let mut r = Rows { rows, remaining: JOB_AUDIT_RESTORE_MAX };
        // ---- fixed header and book ----
        let (root, h): (Digest, JobHeader) = decode(&r.need(TABLE, k(16, &job), 16384, "funded header")?, "funded header")?;
        if h.version != 39
            || h.id != job
            || h.original.job() != job
            || h.root() != root
            || h.book_bytes > 16384
            || h.plan.root() != h.plan_root
            || h.plan_root != h.original.shared_plan
            || h.original.policy != p.root()
            || h.original.admitted > daa
        {
            return Err("audit39 original funded fixed header differs".into());
        }
        let raw = r.need(TABLE, k(17, &job), h.book_bytes as usize, "book")?;
        if raw.len() != h.book_bytes as usize {
            return Err("audit39 original book exact extent differs".into());
        }
        let book: NativeJobBookV39 = decode(&raw, "book")?;
        if book.root() != h.book || book.id() != job || book.original() != &h.original || book.terminal() != h.terminal {
            return Err("audit39 original book relation differs".into());
        }
        // ---- class: live rows, or an archived body bound to its header ----
        let mut budget = r.remaining;
        let class = super::super::palw_native_source_fold_v39::restore_audited_class(rows, &p, h.original.class, &mut budget, archive)?;
        r.remaining = budget;
        if class.root() != h.original.class || class.source().root() != h.original.source || h.class_bytes != 262 {
            return Err("audit39 original class/source identity differs".into());
        }
        // ---- producer consent: live row 27, or the exact archived row ----
        let collected = match r.get(TABLE, k(super::collect::COLLECTED, &job), 2048)? {
            Some(raw) => {
                let (root, c): (Digest, NativeCollectedJobV39) = decode(&raw, "collection record")?;
                if root != c.root()
                    || c.version != 39
                    || c.job != job
                    || c.funding != h.original.root()
                    || c.terminal_book != book.root()
                    || c.consent != h.original.consent
                    || c.class != h.original.class
                    || c.collected > daa
                {
                    return Err("audit39 original collection record differs".into());
                }
                Some(c)
            }
            None => None,
        };
        let consent_raw = match (r.get(TABLE, k(27, &job), NativeConsentWireV39::MAX_BYTES + 16384)?, &collected) {
            (Some(raw), None) => raw,
            (Some(_), Some(_)) => return Err("audit39 collected job still holds its consent row".into()),
            (None, Some(c)) => {
                let raw = archive
                    .ok_or("audit39 collected job requires its archived original consent")?
                    .consent(job, (c.consent_bytes as usize).min(r.remaining))?
                    .ok_or("audit39 archived original consent absent")?;
                if raw.len() != c.consent_bytes as usize || native_consent_body_digest_v39(&raw) != c.consent_body {
                    return Err("audit39 archived consent is not the collected original row".into());
                }
                r.remaining = r.remaining.checked_sub(raw.len()).ok_or("audit39 archived consent budget exhausted")?;
                raw
            }
            (None, None) => return Err("audit39 original consent row absent and job not collected".into()),
        };
        let consent: NativeConsentV39 = decode(&consent_raw, "producer consent")?;
        drop(consent_raw);
        if consent.root() != h.original.consent
            || consent.producer != h.original.producer
            || consent.wire.requester != h.original.requester
            || consent.wire.class != h.original.class
            || consent.wire.reward != h.original.reward
            || consent.wire.requester_operator != h.original.requester_operator
            || consent.wire.producer_operator != h.original.producer_operator
            || consent.interval != h.original.interval
            || consent.checked_maximum.root() != h.original.maximum_contract
        {
            return Err("audit39 original producer consent relation differs".into());
        }
        let tomb: (Digest, Digest, Digest) = decode(&r.need(TABLE, k(20, &consent.root()), 256, "consumed consent")?, "consumed consent")?;
        if tomb != (job, h.original.root(), consent.root()) {
            return Err("audit39 once-consumed consent tombstone differs".into());
        }
        // ---- maximum, quota, complete contract and temporal plan ----
        let wire = misaka_palw_kernel::flat_job39::JobMaximumV39::from_bytes(&consent.wire.maximum)?;
        let material = misaka_palw_kernel::flat_job39::MaterialManifestV39::from_bytes(&consent.wire.material)?;
        let maximum = CheckedJobMaximumV39::check(class.source(), &wire, &material)?;
        let quota = derive_quota_v39(class.source(), &maximum, class.root(), p.root(), class.wire().max_positions, 32)?;
        let contract = &consent.checked_maximum;
        let raw = r.need(QUOTA, b(&(39u8, job, 0u8)), 16384, "complete maximum contract")?;
        let stored: NativeMaximumContractV39 = decode(&raw, "complete maximum contract")?;
        let temporal = NativeTemporalV39::derive(&p, quota.source().requests_per_owner)?;
        if stored != *contract
            || maximum.root() != h.original.maximum
            || maximum.maximum_positions() != h.original.maximum_positions
            || maximum.input_manifest().root() != h.original.input_manifest
            || maximum.request().requester != palw_kernel_bond_id_v1(&h.original.requester)
            || maximum.request().producer != palw_kernel_bond_id_v1(&h.original.producer)
            || maximum.request().nonce != h.original.nonce
            || quota.root() != h.original.quota
            || contract.version != 39
            || contract.class != class.root()
            || contract.source != class.source().root()
            || contract.maximum != maximum.root()
            || contract.policy != p.root()
            || contract.class_scope != class.scope().root()
            || contract.class_resources != class.resources().root()
            || contract.requested_scope != maximum.resources().scope().root()
            || contract.requested_resources != maximum.resources().root()
            || contract.input_manifest != maximum.input_manifest().root()
            || contract.quota != quota.root()
            || contract.shared_plan != quota.plan().record().root()
            || contract.source_plan != quota.source().root()
            || contract.paged_plan != quota.paged().root()
            || contract.physical != p.physical_service
            || contract.temporal != temporal
            || contract.final_policy != p.final_policy
            || contract.final_authority != final_authority_v39(&p)
            || contract.final_basis.root() != h.original.final_basis
            || contract.transport != h.original.transport
            || contract.queue != h.original.queue
            || contract.final_basis.class != class.root()
            || contract.final_basis.program != class.source().program_root()
            || contract.final_basis.inventory != class.source().inventory_root()
            || contract.final_basis.real_parameter != class.source().parameters().root()
            || contract.final_basis.original_max_scope != class.scope().root()
            || contract.final_basis.original_max_resource != class.resources().root()
            || contract.final_basis.execution_policy != p.execution.execution.root()
            || contract.final_basis.quota_policy != p.execution.quota.root()
            || contract.final_basis.source_protocol != p.source_protocol39
            || contract.final_basis.verifier_profile != misaka_palw_kernel::flat_resources39::ordinary_court_id_v39()
            || contract.final_basis.physical_service != p.physical_service
            || contract.final_basis.original_producer_reserve_sompi != p.execution.execution.required_producer_reserve
            || h.original.commit_until != h.original.admitted.saturating_add(temporal.commit_window)
            || h.original.challenge_window != temporal.challenge_window
            || h.original.final_tail != temporal.final_tail
            || h.original.deadline != h.original.admitted.saturating_add(temporal.total)
        {
            return Err("audit39 re-derived maximum/quota/contract/temporal plan differs from the original funding".into());
        }
        book.check(&class, &maximum, &quota, &p)?;
        // ---- original requester nonce, debit receipt ----
        let raw = r.need(NONCE, b(&h.original.requester_operator), 72, "requester nonce counter")?;
        if raw.len() != 72 {
            return Err("audit39 original nonce fixed extent".into());
        }
        let counter: crate::palw_flat_execution_v11::NativeRequesterNonceRowV11 = decode(&raw, "requester nonce counter")?;
        let used = misaka_palw_kernel::flat_execution11::canonical_nonce_v11(&h.original.nonce)?
            .checked_add(1)
            .ok_or("audit39 spent nonce overflow")?;
        if counter.operator != h.original.requester_operator || counter.next_nonce < used {
            return Err("audit39 original requester nonce was not spent".into());
        }
        let (_, receipt, block, admitted): (Digest, Digest, Digest, u64) =
            decode(&r.need(TABLE, k(25, &job), 512, "funding receipt")?, "funding receipt")?;
        if receipt != h.original.debit_receipt || block != h.original.admitted_block || admitted != h.original.admitted {
            return Err("audit39 original actual escrow debit receipt differs".into());
        }
        // ---- accepted Commit and actual EOS ----
        if let Some(claim) = book.claim() {
            let raw = r.need(TABLE, k(28, &job), NativeCommitWireV39::MAX_BYTES, "accepted Commit")?;
            if raw.len() != h.post_bytes as usize {
                return Err("audit39 original accepted Commit extent differs".into());
            }
            let post = NativeCommitWireV39::from_bytes(&raw, h.original.maximum_positions)?;
            let (tokens, actual) = maximum.check_actual(class.source(), &post.generated)?;
            if post.job != job
                || post.claim.root() != claim.claim
                || post.claim.positions != claim.positions
                || tokens.len() != claim.positions as usize
                || post.claim.input_binding != claim.input_binding
                || actual.scope().root() != claim.actual_scope
                || actual.root() != claim.actual_resources
                || object_id(b"misaka-palw/native/original-stopped-generation/v39", &post.generated) != claim.generated
            {
                return Err("audit39 accepted Commit/actual EOS/resources differ".into());
            }
        }
        // ---- once-only credit ----
        let credit = match book.settlement() {
            None => {
                if r.get(TABLE, k(23, &job), 1024)?.is_some() {
                    return Err("audit39 unsettled job holds a single credit".into());
                }
                None
            }
            Some(s) => {
                let (root, c): (Digest, NativeSingleCreditV39) =
                    decode(&r.need(TABLE, k(23, &job), 1024, "single credit")?, "single credit")?;
                let (recipient, payout) = if s.producer_credit != 0 {
                    (h.original.producer, h.producer_payout)
                } else {
                    (h.original.requester, h.requester_payout)
                };
                let queue = Hash64::from_bytes(object_id(
                    b"misaka-palw/native/original-single-credit-queue/v39",
                    &(job, h.original.root(), s.root(), recipient, payout),
                ));
                if root != c.root()
                    || c.version != 39
                    || c.job != job
                    || c.funding != book.funding()
                    || c.terminal != book.root()
                    || c.settlement != s.root()
                    || c.claim != s.claim
                    || c.recipient != recipient
                    || c.payout != payout
                    || c.queue != queue
                    || Some(c.amount) != s.producer_credit.checked_add(s.requester_credit)
                    || c.amount != h.original.reward
                    || c.established != s.established
                    || s.established > daa
                {
                    return Err("audit39 original single credit/queue identity differs".into());
                }
                Some(c)
            }
        };
        // ---- Master binding ----
        let (master, stage) = match r.get(TABLE, k(84, &job), 2048)? {
            Some(raw) => {
                let (m, c): (Digest, NativeMasterFundedComponentV39) = decode(&raw, "Master binding")?;
                if m == [0; 64]
                    || maximum.input_manifest().master != m
                    || c.job != job
                    || c.funding != h.original.root()
                    || c.maximum != h.original.maximum
                    || c.maximum_contract != h.original.maximum_contract
                    || c.input_binding != h.original.input_manifest
                    || c.requester_operator != h.original.requester_operator
                    || c.producer_operator != h.original.producer_operator
                    || c.admitted != h.original.admitted
                {
                    return Err("audit39 original Master component binding differs".into());
                }
                (m, Some(c.stage))
            }
            None => {
                if maximum.input_manifest().master != [0; 64] {
                    return Err("audit39 Master-bound job lacks its original funded binding".into());
                }
                ([0; 64], None)
            }
        };
        Ok(CheckedNativeJobAuditV39 {
            book,
            maximum,
            credit,
            state: rows.state_root(),
            auxiliary: rows.root(),
            daa,
            master,
            stage,
        })
    }

    /// The complete Master Final audit: the source/request view and EVERY
    /// funded component's independent job audit, all from the same anchored
    /// stream, joined by `derive_native_master_final_audit_v39` (every stage,
    /// actual Fund, original claim, settlement, unique queue and credit set).
    /// `Ok(None)` while the aggregate Final row does not exist yet.
    pub fn native_master_final_audit_from_stream_v39<R: NativeAuditRowReaderV39>(
        rows: &VerifiedNativeAuxRowsV39<R>,
        anchor: &CheckedNativeStateAnchorV39,
        parameters: &crate::config::params::Params,
        master: Digest,
        archive: Option<&dyn NativeOriginalSourceArchiveV39>,
    ) -> Result<Option<CheckedNativeMasterFinalAuditV39>, String> {
        let source = Self::native_master_sources_from_stream_with_archive_v39(rows, anchor, parameters, master, archive)?;
        let mut jobs = Vec::with_capacity(source.funded().len());
        for funded in source.funded() {
            let Some(funded) = funded else {
                return Ok(None);
            };
            jobs.push(Self::native_job_audit_from_stream_v39(rows, anchor, parameters, funded.job, archive)?);
        }
        derive_native_master_final_audit_v39(rows, anchor, parameters, std::sync::Arc::new(source), &jobs)
    }
}
