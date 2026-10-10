//! **The ledger's canonical state and its versioned root** (what a consensus header would commit).
//!
//! The state of [`KernelLedgerV1`] is its configuration (policy, kernel schedule, the descriptors this binary implements), its clock
//! and burn checksum, and eight keyed collections — bonds, classes, pipeline classes, jobs, pipeline jobs, claims, demands, served
//! positions — plus the set of attested artifact roots. Nothing else is state: **events are per-block receipts the consumer stores**,
//! and the block's adjudication budget is block-local scratch.
//!
//! ```text
//! root = H(LEDGER_ROOT_DOMAIN_V1; borsh(StateRootPartsV1))
//! StateRootPartsV1 = { version, header, bonds, classes, pipeline_classes, jobs, pipeline_jobs, claims, demands, served, attested }
//! header          = H(HEADER; borsh(LedgerHeaderV1 { version, policy, config_root, daa, burned }))
//! <collection>    = H_domain(count_le64 ‖ for each (key, row) in key order: H(LEAF; u32 len(key) ‖ key ‖ row))   key/row = borsh
//! ```
//!
//! Every row is Borsh with declared discriminants; `BTreeMap` ordering makes the encoding deterministic; the derived caches of a
//! class row (the decoded program, the descriptor, the gate's bounds) are *not* encoded — a class is its registration payload
//! ([`ClassRecordV1`] / [`PipelineClassRecordV1`]), and decoding a program is a pure function of its bytes. A consumer that caches
//! per-collection leaf digests only rehashes the rows a block touched; this reference implementation recomputes everything.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::hash::{Digest, finish, id, keyed, object_id};
use crate::ledger::{ClassRowV1, KernelLedgerV1, LedgerPolicyV1, PipelineClassRowV1};
use crate::pipeline::PipelinePlanV1;
use crate::plan::VerificationPlanV1;
use crate::trace::ParamCommitmentsV1;

pub const LEDGER_STATE_VERSION_V1: u16 = 1;
pub const LEDGER_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/ledger-state-root/v1";
pub const LEDGER_HEADER_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/ledger-header/v1";
pub const LEDGER_CONFIG_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/ledger-config/v1";
pub const LEDGER_LEAF_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/ledger-leaf/v1";

/// A single-program class as the ledger stores it: its registration payload (the descriptor by digest, the program's bytes, the plan
/// and the artifact's commitments). The network and ruleset are the policy's.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ClassRecordV1 {
    pub descriptor: Digest,
    pub program_bytes: Vec<u8>,
    pub plan: VerificationPlanV1,
    pub param_commitments: ParamCommitmentsV1,
}

/// A pipeline class as the ledger stores it.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PipelineClassRecordV1 {
    pub descriptor: Digest,
    pub pipeline_bytes: Vec<u8>,
    pub program_bytes: Vec<Vec<u8>>,
    pub plan: PipelinePlanV1,
    pub param_commitments: Vec<ParamCommitmentsV1>,
    pub decode: Option<crate::job::DecodeRuleV1>,
}

impl ClassRowV1 {
    pub fn record(&self) -> ClassRecordV1 {
        ClassRecordV1 {
            descriptor: self.descriptor.digest(),
            program_bytes: self.program_bytes.clone(),
            plan: self.plan.clone(),
            param_commitments: self.param_commitments.clone(),
        }
    }
}

impl PipelineClassRowV1 {
    pub fn record(&self) -> PipelineClassRecordV1 {
        PipelineClassRecordV1 {
            descriptor: self.descriptor.digest(),
            pipeline_bytes: self.pipeline_bytes.clone(),
            program_bytes: self.program_bytes.clone(),
            plan: self.plan.clone(),
            param_commitments: self.param_commitments.clone(),
            decode: self.binding.decode,
        }
    }
}

/// The scalar part of the state.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct LedgerHeaderV1 {
    pub version: u16,
    pub policy: LedgerPolicyV1,
    /// The kernel schedule and the implemented descriptors (order-independent): two nodes that disagree on them disagree on the root.
    pub config_root: Digest,
    pub daa: u64,
    pub burned: u64,
}

/// What the state root commits: one digest per collection.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StateRootPartsV1 {
    pub version: u16,
    pub header: Digest,
    pub bonds: Digest,
    pub classes: Digest,
    pub pipeline_classes: Digest,
    pub jobs: Digest,
    pub pipeline_jobs: Digest,
    pub claims: Digest,
    pub demands: Digest,
    pub served: Digest,
    pub attested_artifacts: Digest,
    pub job_claims: Digest,
    pub seals: Digest,
    /// GAP-R7: the accusers' proof seals (in the root since the G14-R4 fix).
    pub proof_seals: Digest,
    /// GAP-5: the posters' job escrows that fund Final rewards (in the root since the G14-R4 fix).
    pub job_escrows: Digest,
    /// The demand bonds of served positions awaiting their fate (in the root since G14-R4).
    pub served_demands: Digest,
}

impl StateRootPartsV1 {
    pub fn root(&self) -> Digest {
        object_id(LEDGER_ROOT_DOMAIN_V1, self)
    }
}

/// The root of one keyed collection: its size, then each `(key, row)` leaf in key order.
pub(crate) fn collection_root<'a, K: BorshSerialize + 'a, V: BorshSerialize + 'a>(
    domain: &[u8],
    len: usize,
    rows: impl Iterator<Item = (&'a K, &'a V)>,
) -> Digest {
    let mut s = keyed(domain);
    s.update(&(len as u64).to_le_bytes());
    for (k, v) in rows {
        let key = borsh::to_vec(k).expect("in-memory borsh");
        let mut leaf = (key.len() as u32).to_le_bytes().to_vec();
        leaf.extend_from_slice(&key);
        borsh::to_writer(&mut leaf, v).expect("in-memory borsh");
        s.update(&id(LEDGER_LEAF_DOMAIN_V1, &leaf));
    }
    finish(s)
}

impl KernelLedgerV1 {
    /// The kernel schedule and implemented descriptors, order-independent.
    pub fn config_root(&self) -> Digest {
        let mut schedule = self.schedule.entries.clone();
        schedule.sort_by_key(|(d, _)| *d);
        let mut known: Vec<Digest> = self.known.iter().map(|d| d.digest()).collect();
        known.sort();
        object_id(LEDGER_CONFIG_DOMAIN_V1, &(schedule, known))
    }

    pub fn header(&self) -> LedgerHeaderV1 {
        LedgerHeaderV1 {
            version: LEDGER_STATE_VERSION_V1,
            policy: self.policy,
            config_root: self.config_root(),
            daa: self.daa,
            burned: self.burned,
        }
    }

    /// One digest per collection (a consumer that caches leaves recomputes only the collections a block touched).
    pub fn root_parts(&self) -> StateRootPartsV1 {
        let d = |name: &str| format!("misaka-palw/kernel/ledger-collection/{name}/v1").into_bytes();
        let classes: std::collections::BTreeMap<Digest, ClassRecordV1> = self.classes.iter().map(|(k, c)| (*k, c.record())).collect();
        let pipeline_classes: std::collections::BTreeMap<Digest, PipelineClassRecordV1> =
            self.pipeline_classes.iter().map(|(k, c)| (*k, c.record())).collect();
        StateRootPartsV1 {
            version: LEDGER_STATE_VERSION_V1,
            header: object_id(LEDGER_HEADER_DOMAIN_V1, &self.header()),
            bonds: collection_root(&d("bonds"), self.bonds.len(), self.bonds.iter()),
            classes: collection_root(&d("classes"), classes.len(), classes.iter()),
            pipeline_classes: collection_root(&d("pipeline-classes"), pipeline_classes.len(), pipeline_classes.iter()),
            jobs: collection_root(&d("jobs"), self.jobs.len(), self.jobs.iter()),
            pipeline_jobs: collection_root(&d("pipeline-jobs"), self.pipeline_jobs.len(), self.pipeline_jobs.iter()),
            claims: collection_root(&d("claims"), self.claims.len(), self.claims.iter()),
            demands: collection_root(&d("demands"), self.demands.len(), self.demands.iter()),
            served: collection_root(&d("served"), self.served.len(), self.served.iter()),
            attested_artifacts: collection_root(
                &d("attested-artifacts"),
                self.attested_artifacts.len(),
                self.attested_artifacts.iter().map(|k| (k, &())),
            ),
            job_claims: collection_root(&d("job-claims"), self.job_claims.len(), self.job_claims.iter()),
            seals: collection_root(&d("seals"), self.seals.len(), self.seals.iter()),
            proof_seals: collection_root(&d("proof-seals"), self.proof_seals.len(), self.proof_seals.iter()),
            job_escrows: collection_root(&d("job-escrows"), self.job_escrows.len(), self.job_escrows.iter()),
            served_demands: collection_root(&d("served-demand-bonds"), self.served_demands.len(), self.served_demands.iter()),
        }
    }

    /// **The state root**: two nodes agree iff their roots do. Versioned, canonical, over per-collection roots.
    ///
    /// A ledger with no OPV policy (RFC-0015 dormant: [`crate::opv::OpvStateV1::is_dormant`]) has the historical root,
    /// [`StateRootPartsV1::root`], byte for byte. Once an OPV policy is set the root is [`crate::opv::StateRootPartsV2`]: the
    /// historical root of the whole Panel-licensed route, the policy, the OPV classes and the OPV claim rows.
    pub fn root(&self) -> Digest {
        let base = if self.opv.is_dormant() { self.root_parts().root() } else { self.root_parts_v2().root() };
        // K2-TIR-v4 (tables 20 and 21): an extension only once either holds a row, so every older root is unchanged; then
        // RFC-0004 Part II's typed tables, each only when non-empty (`crate::rows::root_of_rows` composes them the same way).
        let base = if self.tiled_jobs.is_empty() && self.seg_progress.is_empty() {
            base
        } else {
            let d = |name: &str| format!("misaka-palw/kernel/ledger-collection/{name}/v1").into_bytes();
            crate::rows::seg_root_extension_v1(
                &base,
                &collection_root(&d("tiled-jobs"), self.tiled_jobs.len(), self.tiled_jobs.iter()),
                &collection_root(&d("seg-progress"), self.seg_progress.len(), self.seg_progress.iter()),
            )
        };
        // RFC-0004 Part II: the typed tables, each only when non-empty.
        let base = crate::spec::typed_root_v1(base, &self.typed_root_parts());
        // OPV-BOOT GAP-B1a / C4R4 F-C4R4-08 (tables 25, 26 and 18): an extension only once any holds a row, so every older root is
        // unchanged.
        if self.claim_beacon_salts.is_empty() && self.forfeited_claim_seals.is_empty() && self.job_posters.is_empty() {
            return base;
        }
        let d = |name: &str| format!("misaka-palw/kernel/ledger-collection/{name}/v1").into_bytes();
        crate::rows::beacon_seal_root_extension_v1(
            &base,
            &collection_root(&d("claim-beacon-salts"), self.claim_beacon_salts.len(), self.claim_beacon_salts.iter()),
            &collection_root(&d("forfeited-claim-seals"), self.forfeited_claim_seals.len(), self.forfeited_claim_seals.iter()),
            &collection_root(&d("job-posters"), self.job_posters.len(), self.job_posters.iter()),
        )
    }

    /// RFC-0004 Part II: `(table, collection root)` of every NON-EMPTY typed table, in root order (empty for an untyped ledger).
    pub fn typed_root_parts(&self) -> Vec<(u8, Digest)> {
        let d = |name: &str| format!("misaka-palw/kernel/ledger-collection/{name}/v1").into_bytes();
        let t = &self.typed;
        let mut out = Vec::new();
        if !t.classes.is_empty() {
            let records: std::collections::BTreeMap<Digest, crate::spec::ComputationSpecV1> =
                t.classes.iter().map(|(k, c)| (*k, c.spec.clone())).collect();
            out.push((crate::rows::TABLE_SPEC_CLASSES_V1, collection_root(&d("spec-classes"), records.len(), records.iter())));
        }
        if !t.jobs.is_empty() {
            out.push((crate::rows::TABLE_SPEC_JOBS_V1, collection_root(&d("spec-jobs"), t.jobs.len(), t.jobs.iter())));
        }
        if !t.lines.is_empty() {
            out.push((crate::rows::TABLE_MEMORY_LINES_V1, collection_root(&d("memory-lines"), t.lines.len(), t.lines.iter())));
        }
        out
    }
}
