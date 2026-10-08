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
}

impl StateRootPartsV1 {
    pub fn root(&self) -> Digest {
        object_id(LEDGER_ROOT_DOMAIN_V1, self)
    }
}

/// The root of one keyed collection: its size, then each `(key, row)` leaf in key order.
fn collection_root<'a, K: BorshSerialize + 'a, V: BorshSerialize + 'a>(
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
        }
    }

    /// **The state root**: two nodes agree iff their roots do. Versioned, canonical, over per-collection roots.
    pub fn root(&self) -> Digest {
        self.root_parts().root()
    }
}
