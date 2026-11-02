//! A0 namespace reservation. This does not install a native decoder, state row,
//! fork activation, funding lock or credit authority. The native core owns the
//! bounded row values and acceptance; A6 must wire typed restore scanning too.
use crate::identity::hash;
use crate::{
    CONTRACT_REVISION, CONTRACT_VERSION,
    assignment::{AssignmentId, AssignmentRoleV2, rows::AssignmentSettlementLegV2},
    identity::*,
    messages::{MinerResultCommit, SourceKind},
    rows::NativeOutpoint,
};
use borsh::{BorshDeserialize, BorshSerialize};

pub const LIFECYCLE_OBJECT_TAG: u8 = 176;
pub const KERNEL_AUX_TABLE: u8 = 151;
pub const CERTIFICATION_SUBJECT_PREFIX: u8 = b'P';

/// Local row-kind byte, independent of global lifecycle tag numbers. Append
/// future kinds; never renumber an existing key or deserialize it as authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum NativeRowKey {
    Result(ResultId),
    ClaimIndex(ClaimId),
    Case(CaseId),
    Witness(WitnessId),
    Coverage(WitnessId),
    SourceUnion {
        case: CaseId,
        kind: SourceKind,
        source: Digest,
    },
    /// Unit receipt identity excludes payer/recipient/Panel/relay/attempt.
    ServiceReceipt {
        case: CaseId,
        kind: SourceKind,
        source: Digest,
        unit: u64,
    },
    Funding(CaseId),
    PacketReceipt {
        witness: WitnessId,
        first_relation: u64,
    },
    JournalHead(CaseId),
    Assignment(AssignmentId),
    AssignmentNonce(NativeOutpoint),
    AssignmentUse(AssignmentId),
    AssignmentFunding(AssignmentId),
    AssignmentSettlement {
        assignment: AssignmentId,
        leg: AssignmentSettlementLegV2,
        party: PrincipalId,
        ordinal: u64,
    },
    AssignmentRole {
        assignment: AssignmentId,
        role: AssignmentRoleV2,
        bond: NativeOutpoint,
    },
    AssignmentJournalHead(AssignmentId),
    AssignmentJournalFrame {
        assignment: AssignmentId,
        ordinal: u64,
    },
    AssignmentProverFunding {
        assignment: AssignmentId,
        prover: NativeOutpoint,
    },
}
impl NativeRowKey {
    pub fn encode(&self) -> Vec<u8> {
        borsh::to_vec(&(CONTRACT_VERSION, CONTRACT_REVISION, self)).expect("fixed native row key")
    }
}

/// Uses the already-FIRED fork identity and adopted Positive policy, not the
/// fingerprint of an entire future activation schedule. Resolved by native code.
pub fn ruleset_id(fired_fork: Digest, policy: Digest) -> Digest {
    hash(b"misaka/positive/native-ruleset/v2/r1", &(fired_fork, policy))
}
/// Fresh claim-level Gate C subject. A native reader must bind the literal
/// Result row and actual drawn-verifier Certified outcome; never an old S/K/V
/// row, PublicCheckCertified, a saved Pass flag, or a caller boolean.
pub fn certification_subject(result: &MinerResultCommit) -> Digest {
    hash(b"misaka/positive/certification-subject/v2/r1", &(result.claim_id(), result.id()))
}
