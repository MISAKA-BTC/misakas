//! Explicit funded fixed-count text execution carriers. The inner v9 trace
//! remains computation evidence; only this new outer identity binds a funded
//! job and its requester nonce. It supplies no eligibility or money by itself.
use crate::Hash64;
use crate::palw_flat_model_vector_v3::{FLAT_MODEL_VECTOR_MAX_POST_BYTES_V3, vector_id};
use crate::palw_flat_model_vector_v9::{FlatModelVectorPostV9, decode_flat_model_vector_post_v9};
use crate::{palw_kernel_route_v1::PalwKernelRouteStateV1, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::hash::{Digest, object_id};
use misaka_palw_kernel::{
    flat_execution11::{CheckedExecutionClassV11, ExecutionClassRecordV11, ExecutionJobStateV11},
    flat_quota11::{CheckedJobQuotaPlanV11, QuotaPlanRecordV11},
};
use std::io::{Cursor, Error, ErrorKind};

pub const EXECUTION_CLASS_TABLE_V11: u8 = 87;
pub const EXECUTION_JOB_TABLE_V11: u8 = 88;
pub const EXECUTION_REQUEST_INDEX_TABLE_V11: u8 = 89;
pub const EXECUTION_VECTOR_JOB_TABLE_V11: u8 = 90;
pub const EXECUTION_PROTECTED_TABLE_V11: u8 = 91;
pub const EXECUTION_QUOTA_TABLE_V11: u8 = 92;
pub const EXECUTION_POLICY_TABLE_V11: u8 = 93;
pub const EXECUTION_SOURCE_FACT_TABLE_V11: u8 = 94;
pub const EXECUTION_CLASS_OWNER_TABLE_V11: u8 = 95;
pub const EXECUTION_JOB_OWNER_TABLE_V11: u8 = 96;
pub const EXECUTION_JOB_COUNT_TABLE_V11: u8 = 97;
pub const EXECUTION_PREPARATION_TABLE_V11: u8 = 98;
pub const EXECUTION_SEAL_TABLE_V11: u8 = 99;
pub const EXECUTION_SEAL_ACTOR_TABLE_V11: u8 = 100;
pub const EXECUTION_SEAL_COUNT_TABLE_V11: u8 = 101;
pub const EXECUTION_DEMAND_TABLE_V11: u8 = 102;
pub const EXECUTION_DEMAND_SLOT_TABLE_V11: u8 = 103;
pub const EXECUTION_SERVED_TABLE_V11: u8 = 104;
pub const EXECUTION_DEMAND_COUNT_TABLE_V11: u8 = 105;
pub const EXECUTION_CREDIT_TABLE_V11: u8 = 106;
pub const EXECUTION_CLASSES_PER_ACTOR_V11: usize = 8;
pub const EXECUTION_JOBS_PER_REQUESTER_V11: usize = 8;
pub const EXECUTION_LIVE_JOBS_MAX_V11: u32 = 128;
pub const EXECUTION_PREPARATIONS_PER_ACTOR_V11: usize = 8;
pub const EXECUTION_FILING_MAX_BYTES_V11: usize = (1 << 20) + (16 << 10);
pub const EXECUTION_FRAGMENT_MAX_BYTES_V11: usize = 32 << 10;

const POST_PREFIX_BYTES_V11: usize = 2 + 7 * 64;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatExecutionPostV11 {
    pub version: u16,
    pub execution_class: Digest,
    pub job_id: Digest,
    pub requester_nonce: Digest,
    pub job_input_root: Digest,
    pub actual_scope9_root: Digest,
    pub actual_resource10_root: Digest,
    pub quota_plan_root: Digest,
    pub body: FlatModelVectorPostV9,
}
impl FlatExecutionPostV11 {
    pub fn trace_root(&self, candidate9: Digest, binding: Digest) -> Digest {
        object_id(
            b"misaka-palw/flat-funded-execution/trace/v11",
            &(
                self.version,
                self.execution_class,
                self.job_id,
                self.requester_nonce,
                self.job_input_root,
                self.actual_scope9_root,
                self.actual_resource10_root,
                self.quota_plan_root,
                self.body.trace_root(candidate9, binding),
            ),
        )
    }
    pub fn id(&self, candidate9: Digest, binding: Digest) -> Hash64 {
        vector_id(self.trace_root(candidate9, binding))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_kernel::{
        descriptor::k2_tir_v4_descriptor,
        evidence::{EvidenceHeaderV1, SuiteParamsV1},
        seg::SegmentedEvidenceV2,
    };

    fn request() -> FlatExecutionFundV11 {
        FlatExecutionFundV11 {
            version: 11,
            execution_class: [1; 64],
            preparation: [2; 64],
            producer: [3; 64],
            nonce: misaka_palw_kernel::flat_execution11::nonce_bytes_v11(0),
            reward: 20,
            prompt: vec![4, 5],
            generated_count: 3,
        }
    }
    #[test]
    fn hostile_fund_count_wrong_role_and_old_version_refuse_before_prompt_allocation() {
        let request = request();
        let bytes = borsh::to_vec(&request).unwrap();
        assert_eq!(decode_flat_execution_fund_v11(&bytes, 2).unwrap(), request);
        assert_eq!(request.input(false).unwrap().positions, 4);
        assert!(request.input(true).is_err());
        let mut count = bytes.clone();
        let at = 2 + 4 * 64 + 8;
        count[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_flat_execution_fund_v11(&count, u32::MAX).is_err());
        assert!(decode_flat_execution_fund_v11(&bytes, 1).is_err());
        let mut old = bytes.clone();
        old[..2].copy_from_slice(&9u16.to_le_bytes());
        assert!(decode_flat_execution_fund_v11(&old, 2).is_err());
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(decode_flat_execution_fund_v11(&trailing, 2).is_err());
        assert!(decode_flat_execution_fund_v11(&bytes[..bytes.len() - 1], 2).is_err());
    }
    #[test]
    fn operator_nonce_counter_rejects_old_future_noncanonical_and_exhausted_nonces() {
        use misaka_palw_kernel::flat_execution11::{native_requester_nonce_counter_root_v11, nonce_bytes_v11};
        let row = NativeRequesterNonceRowV11 { operator: [9; 64], next_nonce: 5 };
        assert_eq!(row.root(), native_requester_nonce_counter_root_v11(&row.operator, 5));
        assert_eq!(borsh::to_vec(&row).unwrap().len(), 72);
        let next = row.advance(&nonce_bytes_v11(5)).unwrap();
        assert_eq!(next.next_nonce, 6);
        assert_ne!(next.root(), row.root());
        assert!(row.advance(&nonce_bytes_v11(4)).is_err());
        assert!(row.advance(&nonce_bytes_v11(6)).is_err());
        assert!(next.advance(&nonce_bytes_v11(5)).is_err());
        let mut forged = nonce_bytes_v11(5);
        forged[8] = 1;
        assert!(row.advance(&forged).is_err());
        assert!(
            NativeRequesterNonceRowV11 { operator: row.operator, next_nonce: u64::MAX }.advance(&nonce_bytes_v11(u64::MAX)).is_err()
        );
    }
    fn post() -> FlatExecutionPostV11 {
        let descriptor = k2_tir_v4_descriptor();
        let prompt = vec![4];
        let segments = vec![[5; 64]];
        FlatExecutionPostV11 {
            version: 11,
            execution_class: [1; 64],
            job_id: [2; 64],
            requester_nonce: [3; 64],
            job_input_root: [4; 64],
            actual_scope9_root: [5; 64],
            actual_resource10_root: [6; 64],
            quota_plan_root: [7; 64],
            body: FlatModelVectorPostV9 {
                version: 9,
                profile: [8; 64],
                court_set: [9; 64],
                scope_root: [10; 64],
                replay_root: [11; 64],
                resource_root: [12; 64],
                trace: crate::palw_model_vector_v2::ModelVectorPostV2 {
                    version: 2,
                    prompt,
                    max_new_tokens: 1,
                    generated: vec![6],
                    evidence: SegmentedEvidenceV2 {
                        version: 2,
                        header: EvidenceHeaderV1 {
                            network_domain: [1; 64],
                            ruleset_digest: [2; 64],
                            class_binding_id: [3; 64],
                            program_root: [4; 64],
                            artifact_root: [5; 64],
                            plan_root: [6; 64],
                        },
                        job_input_root: [7; 64],
                        positions: 1,
                        segment_len: misaka_palw_kernel::seg::SEG_LEN_V4,
                        claim_root: misaka_palw_kernel::seg::claim_root_v1(1, &segments),
                        suite: SuiteParamsV1::of(&descriptor),
                    },
                    segment_roots: segments,
                    implementation_roots: [[9; 64]; 3],
                },
            },
        }
    }
    #[test]
    fn funded_outer_identity_refuses_old9_and_cannot_alias_two_jobs_or_nonces() {
        let a = post();
        let bytes = borsh::to_vec(&a).unwrap();
        assert_eq!(decode_flat_execution_post_v11(&bytes).unwrap(), a);
        assert!(decode_flat_execution_post_v11(&borsh::to_vec(&a.body).unwrap()).is_err());
        let mut b = a.clone();
        b.job_id[0] ^= 1;
        assert_eq!(a.body, b.body);
        assert_ne!(a.trace_root([4; 64], [5; 64]), b.trace_root([4; 64], [5; 64]));
        assert_ne!(a.id([4; 64], [5; 64]), b.id([4; 64], [5; 64]));
        b = a.clone();
        b.requester_nonce[0] ^= 1;
        assert_ne!(a.id([4; 64], [5; 64]), b.id([4; 64], [5; 64]));
        b = a.clone();
        b.quota_plan_root[0] ^= 1;
        assert_ne!(a.id([4; 64], [5; 64]), b.id([4; 64], [5; 64]));
    }
    #[test]
    fn filing_wrapper_has_no_source_absence_kind_and_count_cannot_allocate() {
        let filing = FlatExecutionFilingV11 { version: 11, kind: 1, body: vec![3; 8] };
        let bytes = borsh::to_vec(&filing).unwrap();
        assert_eq!(decode_flat_execution_filing_v11(&bytes, bytes.len() as u64).unwrap(), filing);
        let mut wrong = bytes.clone();
        wrong[2] = 2;
        assert!(decode_flat_execution_filing_v11(&wrong, 100).is_err());
        wrong = bytes.clone();
        wrong[3..7].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_flat_execution_filing_v11(&wrong, 100).is_err());
        assert!(decode_flat_execution_filing_v11(&bytes, bytes.len() as u64 - 1).is_err());
        assert!(decode_flat_execution_filing_v11(&bytes[..bytes.len() - 1], 100).is_err());
    }
    #[test]
    fn salted_seal_commits_exact_job_actor_scope_and_selected_filing_root() {
        let job = [1; 64];
        let vector = [2; 64];
        let actor = [3; 64];
        let scope = [4; 64];
        let root = [5; 64];
        let salt = [6; 64];
        let seal = execution_proof_seal_v11(&job, &vector, &actor, &scope, &root, &salt);
        assert_ne!(seal, execution_proof_seal_v11(&[7; 64], &vector, &actor, &scope, &root, &salt));
        assert_ne!(seal, execution_proof_seal_v11(&job, &vector, &[7; 64], &scope, &root, &salt));
        assert_ne!(seal, execution_proof_seal_v11(&job, &vector, &actor, &[7; 64], &root, &salt));
        assert_ne!(seal, execution_proof_seal_v11(&job, &vector, &actor, &scope, &[7; 64], &salt));
        assert_ne!(seal, execution_proof_seal_v11(&job, &vector, &actor, &scope, &root, &[7; 64]));
    }
}

/// Both courts remain available for the entire original claim liability. A
/// missing original-model source has no filing kind. The wire only carries a
/// bounded alleged counterexample; the native court establishes the judgment.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatExecutionFilingV11 {
    pub version: u16,
    /// 0: complete mixed computation court9; 1: original R/PC tile court2.
    pub kind: u8,
    pub body: Vec<u8>,
}
/// New bounded original-PC header counterexample. Its explicit wire version
/// leaves the legacy v11 mixed/source-tile filing grammar unchanged.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeSourceHeaderFaultV16 {
    pub param: u16,
    pub layer_code: u32,
    pub header: misaka_palw_kernel::flat_quota11::source16::opening::HeaderV16,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct NativeSourceHeaderFilingV16 {
    pub version: u16,
    pub kind: u8,
    pub body: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeVerifiedHeaderFactV16 {
    pub version: u16,
    pub binding: Digest,
    pub program: Digest,
    pub pc: Digest,
    pub proof_root: Digest,
    pub established: u64,
    pub finder: Digest,
    pub fault: NativeSourceHeaderFaultV16,
}
pub fn decode_native_source_header_filing_v16(bytes: &[u8], cap: u64) -> Result<NativeSourceHeaderFilingV16, String> {
    if bytes.len() != 7 + 2 + 4 + 162
        || bytes.len() as u64 > cap
        || bytes[..2] != 16u16.to_le_bytes()
        || bytes[2] != 2
        || u32::from_le_bytes(bytes[3..7].try_into().unwrap()) as usize != bytes.len() - 7
    {
        return Err("source header filing16 exact framing".into());
    }
    Ok(NativeSourceHeaderFilingV16 { version: 16, kind: 2, body: bytes[7..].to_vec() })
}
pub fn decode_flat_execution_filing_v11(bytes: &[u8], admitted_cap: u64) -> Result<FlatExecutionFilingV11, String> {
    if bytes.len() < 7
        || bytes.len() > EXECUTION_FILING_MAX_BYTES_V11
        || bytes.len() as u64 > admitted_cap
        || bytes[..2] != 11u16.to_le_bytes()
        || bytes[2] > 1
    {
        return Err("funded execution filing exceeds its original grammar/cap".into());
    }
    let count = u32::from_le_bytes(bytes[3..7].try_into().unwrap()) as usize;
    if count == 0 || count != bytes.len() - 7 {
        return Err("funded filing exact body count differs".into());
    }
    Ok(FlatExecutionFilingV11 { version: 11, kind: bytes[2], body: bytes[7..].to_vec() })
}
pub fn execution_proof_root_v11(bytes: &[u8]) -> Digest {
    misaka_palw_kernel::hash::id(b"misaka-palw/native/funded-proof/v11", bytes)
}
pub fn execution_proof_seal_v11(
    job: &Digest,
    vector: &Digest,
    actor: &Digest,
    scope: &Digest,
    proof_root: &Digest,
    salt: &Digest,
) -> Digest {
    object_id(b"misaka-palw/native/funded-own-proof-seal/v11", &(11u16, *job, *vector, *actor, *scope, *proof_root, *salt))
}

/// The complete original paid record is journaled. A different operator's
/// bond, job, proof, source scope or clock cannot consume this paid entitlement.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeExecutionSealV11 {
    pub version: u16,
    pub job_id: Digest,
    pub vector: Digest,
    pub claim_id: Digest,
    pub original_book_root: Digest,
    pub funded_job_root: Digest,
    pub bond: PalwBondKeyV2,
    pub actor: Digest,
    pub operator: Digest,
    pub producer_operator: Digest,
    pub class_scope9_root: Digest,
    pub proof_root: Digest,
    pub seal: Digest,
    pub posted_daa: u64,
    pub mature_daa: u64,
    pub reveal_until: u64,
    pub horizon_daa: u64,
    pub original_paid_debit: u64,
    pub original_paid_receipt: Digest,
    pub original_paid_record_root: Digest,
    pub spent: bool,
}
impl NativeExecutionSealV11 {
    pub fn id(&self) -> Digest {
        object_id(
            b"misaka-palw/native/funded-own-seal-record/v11",
            &(self.version, self.job_id, self.vector, self.actor, self.proof_root, self.seal, self.posted_daa),
        )
    }
    pub fn checked_original_paid_root(&self) -> Result<Digest, String> {
        use misaka_palw_kernel::flat_quota11::{NativePaidSealAnchorV11, native_paid_seal_record_root_v11};
        let n = NativePaidSealAnchorV11 {
            job_id: self.job_id,
            funded_job_root: self.funded_job_root,
            book_root: self.original_book_root,
            claim_id: self.claim_id,
            vector_id: self.vector,
            seal_actor: self.actor,
            seal_operator_id: self.operator,
            producer_operator_id: self.producer_operator,
            proof_root: self.proof_root,
            posted_daa: self.posted_daa,
            matures_daa: self.mature_daa,
            horizon_daa: self.horizon_daa,
            original_paid_seal_root: self.original_paid_record_root,
            original_debit: self.original_paid_debit,
            original_debit_receipt: self.original_paid_receipt,
            shared_live_seals: 0,
            shared_actor_live_seals: 0,
        };
        let root = native_paid_seal_record_root_v11(&n);
        if self.original_paid_debit == 0
            || self.original_paid_receipt == [0; 64]
            || self.operator == self.producer_operator
            || root != self.original_paid_record_root
        {
            return Err("native own seal original paid money/operator root differs".into());
        }
        Ok(root)
    }
    pub fn check_reveal(&self, actor: &Digest, salt: &Digest, bytes: &[u8], current_daa: u64) -> Result<(), String> {
        if self.checked_original_paid_root().is_err()
            || self.version != 11
            || self.spent
            || self.actor != *actor
            || self.mature_daa > current_daa
            || current_daa >= self.reveal_until
            || self.original_paid_debit == 0
            || execution_proof_root_v11(bytes) != self.proof_root
            || execution_proof_seal_v11(&self.job_id, &self.vector, actor, &self.class_scope9_root, &self.proof_root, salt)
                != self.seal
        {
            return Err("original own funded seal is immature/spent/late or filing identity differs".into());
        }
        Ok(())
    }
}

/// A terminal marker records the original debit and exact credits together.
/// Native creates this row only after its private checked book transition and
/// actual atomic payout-queue writes, never from an alleged settlement carrier.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeExecutionCreditV11 {
    pub job_id: Digest,
    pub original_funded_job_root: Digest,
    pub terminal_book_root: Digest,
    pub settlement_root: Digest,
    pub requester_refund: u64,
    pub producer_credit: u64,
    pub collected_original_slash: u64,
    pub finder: Option<Digest>,
    pub finder_credit: u64,
    pub burned: u64,
    pub established_daa: u64,
}

/// Job-independent original R/PC theorem. Only successful native source-court
/// verification writes this table. A later readonly Close authenticates this
/// immutable exact source namespace and original inclusion clock; a binder's
/// refuted boolean or a caller's claimed hash cannot manufacture the theorem.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeVerifiedSourceFactHeaderV11 {
    pub version: u16,
    pub binding_id: Digest,
    pub descriptor: Digest,
    pub program_root: Digest,
    pub inventory_root: Digest,
    pub parameter_root: Digest,
    pub proof_root: Digest,
    pub established_daa: u64,
    pub finder: Digest,
    pub finder_operator: Digest,
    pub proof_bytes_len: u32,
}
impl NativeVerifiedSourceFactHeaderV11 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/established-original-source-fact/v11", self)
    }
    pub fn established_for(
        &self,
        class: &ExecutionClassRecordV11,
        book: &ExecutionJobStateV11,
        original_producer_operator: &Digest,
    ) -> Result<misaka_palw_kernel::flat_execution11::NativeEstablishedExecutionCourtV11, String> {
        let claim = book.claim().ok_or("source theorem has no funded claim to settle")?;
        if self.version != 11
            || self.binding_id != class.binding_id
            || self.descriptor != class.descriptor
            || self.program_root != class.program_root
            || self.inventory_root != class.inventory_root
            || self.parameter_root != class.parameter_root
            || book.job().class_id != class.id()
            || self.established_daa < claim.committed_daa
            || self.established_daa > claim.retention_until
        {
            return Err("original verified source theorem does not establish this exact retained claim".into());
        }
        Ok(misaka_palw_kernel::flat_execution11::NativeEstablishedExecutionCourtV11 {
            job_id: book.job().id(),
            claim_id: claim.id(),
            vector: claim.vector,
            class_id: class.id(),
            inventory_root: class.inventory_root,
            parameter_root: class.parameter_root,
            established_daa: self.established_daa,
            proof_id: self.proof_root,
            kind: misaka_palw_kernel::flat_execution11::NativeAdjudicationKindV11::SourceMismatch,
            original_finder: self.finder,
            eligible_finder: (self.finder_operator != *original_producer_operator).then_some(self.finder),
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct NativeVerifiedSourceFactV11 {
    pub header: NativeVerifiedSourceFactHeaderV11,
    pub proof_bytes: Vec<u8>,
}

pub fn decode_flat_execution_post_v11(bytes: &[u8]) -> std::io::Result<FlatExecutionPostV11> {
    let bad = || Error::new(ErrorKind::InvalidData, "invalid bounded funded execution11 post");
    if bytes.len() < POST_PREFIX_BYTES_V11 + 258
        || bytes.len() > FLAT_MODEL_VECTOR_MAX_POST_BYTES_V3
        || bytes[..2] != 11u16.to_le_bytes()
    {
        return Err(bad());
    }
    let mut r = Cursor::new(bytes);
    let version = u16::deserialize_reader(&mut r)?;
    let execution_class = Digest::deserialize_reader(&mut r)?;
    let job_id = Digest::deserialize_reader(&mut r)?;
    let requester_nonce = Digest::deserialize_reader(&mut r)?;
    let job_input_root = Digest::deserialize_reader(&mut r)?;
    let actual_scope9_root = Digest::deserialize_reader(&mut r)?;
    let actual_resource10_root = Digest::deserialize_reader(&mut r)?;
    let quota_plan_root = Digest::deserialize_reader(&mut r)?;
    let body = decode_flat_model_vector_post_v9(&bytes[r.position() as usize..])?;
    Ok(FlatExecutionPostV11 {
        version,
        execution_class,
        job_id,
        requester_nonce,
        job_input_root,
        actual_scope9_root,
        actual_resource10_root,
        quota_plan_root,
        body,
    })
}

/// Public request data. The native consumer derives all actual resources and
/// debits original requester coins; no balance, price or verification flag is
/// carried here. The producer must later sign its own first Commit.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatExecutionFundV11 {
    pub version: u16,
    pub execution_class: Digest,
    pub preparation: Digest,
    pub producer: Digest,
    pub nonce: Digest,
    pub reward: u64,
    pub prompt: Vec<u32>,
    pub generated_count: u32,
}
const FUND_FIXED_BYTES_V11: usize = 2 + 4 * 64 + 8 + 4 + 4;
impl FlatExecutionFundV11 {
    pub fn input(&self, encoder: bool) -> Result<misaka_palw_kernel::flat_execution11::ExecutionJobInputV11, String> {
        use misaka_palw_kernel::{flat_execution11::ExecutionJobInputV11, flat_lengths7::FlatJobLengthsV7};
        if self.version != 11 || self.prompt.is_empty() || self.reward == 0 {
            return Err("funded execution11 request version, prompt or reward differs".into());
        }
        let prompt_len = u32::try_from(self.prompt.len()).map_err(|_| "funded prompt count overflow")?;
        let positions = if encoder {
            if self.generated_count != 0 {
                return Err("funded encoder cannot declare generated tokens".into());
            }
            1
        } else {
            if self.generated_count == 0 {
                return Err("funded decoder requires a fixed positive generation count".into());
            }
            prompt_len.checked_add(self.generated_count).and_then(|n| n.checked_sub(1)).ok_or("funded fed positions overflow")?
        };
        Ok(ExecutionJobInputV11 {
            version: 11,
            prompt_root: misaka_palw_kernel::seg::prompt_root_of_ids_v1(&self.prompt),
            prompt_len,
            generated_count: self.generated_count,
            lengths: FlatJobLengthsV7 { token: if encoder { prompt_len } else { positions }, patch: 0, audio: 0 },
            positions,
        })
    }
}
/// Check the fixed class-derived prompt ceiling before allocating its token
/// vector. The outer route cap independently bounds bytes before this call.
pub fn decode_flat_execution_fund_v11(bytes: &[u8], max_prompt: u32) -> std::io::Result<FlatExecutionFundV11> {
    let bad = || Error::new(ErrorKind::InvalidData, "invalid bounded fixed-count funded request11");
    if bytes.len() < FUND_FIXED_BYTES_V11 || bytes.len() > FLAT_MODEL_VECTOR_MAX_POST_BYTES_V3 || bytes[..2] != 11u16.to_le_bytes() {
        return Err(bad());
    }
    let mut r = Cursor::new(bytes);
    let version = u16::deserialize_reader(&mut r)?;
    let execution_class = Digest::deserialize_reader(&mut r)?;
    let preparation = Digest::deserialize_reader(&mut r)?;
    let producer = Digest::deserialize_reader(&mut r)?;
    let nonce = Digest::deserialize_reader(&mut r)?;
    let reward = u64::deserialize_reader(&mut r)?;
    let count = u32::deserialize_reader(&mut r)?;
    if count == 0 || count > max_prompt || reward == 0 {
        return Err(bad());
    }
    let exact = (count as usize).checked_mul(4).and_then(|n| n.checked_add(FUND_FIXED_BYTES_V11)).ok_or_else(bad)?;
    if exact != bytes.len() {
        return Err(bad());
    }
    let mut prompt = Vec::with_capacity(count as usize);
    for _ in 0..count {
        prompt.push(u32::deserialize_reader(&mut r)?);
    }
    let generated_count = u32::deserialize_reader(&mut r)?;
    Ok(FlatExecutionFundV11 { version, execution_class, preparation, producer, nonce, reward, prompt, generated_count })
}

/// One fixed rooted row per requester operator. Native requires exact next
/// canonical nonce and checked increment, also across bond rotations. This row
/// remains after a spent job's full retained book is safely collected.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeRequesterNonceRowV11 {
    pub operator: Digest,
    pub next_nonce: u64,
}
impl NativeRequesterNonceRowV11 {
    pub fn root(&self) -> Digest {
        misaka_palw_kernel::flat_execution11::native_requester_nonce_counter_root_v11(&self.operator, self.next_nonce)
    }
    pub fn advance(&self, nonce: &Digest) -> Result<Self, String> {
        let n = misaka_palw_kernel::flat_execution11::canonical_nonce_v11(nonce)?;
        if n != self.next_nonce {
            return Err("native execution11 requester nonce is duplicate or out of order".into());
        }
        Ok(Self { operator: self.operator, next_nonce: n.checked_add(1).ok_or("native requester nonce exhausted")? })
    }
}

/// Fixed original class header. Candidate9 independently derived the class-max
/// resource root at its admission; native rechecks its full scope and exact
/// source/policy instead of copying a second whole class resource vector.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeExecutionClassJournalV11 {
    pub record: ExecutionClassRecordV11,
    pub original_header_root: Digest,
    pub encoder_input_limit: Option<u32>,
    pub owner: PalwBondKeyV2,
    pub admitted_daa: u64,
    pub primitive_count: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeExecutionClassRowV11 {
    pub header: NativeExecutionClassJournalV11,
}
impl NativeExecutionClassRowV11 {
    pub fn from_checked(class: &CheckedExecutionClassV11, owner: PalwBondKeyV2, admitted_daa: u64) -> Result<Self, String> {
        let primitive_count = u32::try_from(class.view().resources.primitives.len()).map_err(|_| "native resource count overflow")?;
        if class.view().resources.root() != class.record().resource10_root {
            return Err("native class resource record differs from independently checked class".into());
        }
        let encoder_input_limit = class.view().encoder.map(|e| e.l);
        let original_header_root = misaka_palw_kernel::flat_execution11::NativeExecutionClassHeaderV11 {
            version: 11,
            record: class.record().clone(),
            encoder_input_limit,
        }
        .root();
        Ok(Self {
            header: NativeExecutionClassJournalV11 {
                record: class.record().clone(),
                original_header_root,
                encoder_input_limit,
                owner,
                admitted_daa,
                primitive_count,
            },
        })
    }
}
impl NativeExecutionClassJournalV11 {
    pub fn checked_fixed_header(&self) -> Result<misaka_palw_kernel::flat_execution11::CheckedNativeExecutionClassHeaderV11, String> {
        misaka_palw_kernel::flat_execution11::NativeExecutionClassHeaderV11 {
            version: 11,
            record: self.record.clone(),
            encoder_input_limit: self.encoder_input_limit,
        }
        .restore(self.original_header_root)
    }
}

pub fn native_execution_class_header_v11(bytes: &[u8]) -> Result<(NativeExecutionClassJournalV11, usize), String> {
    let mut r = Cursor::new(bytes);
    let header =
        NativeExecutionClassJournalV11::deserialize_reader(&mut r).map_err(|_| "invalid fixed original execution class header")?;
    let at = r.position() as usize;
    if header.record.version != 11
        || header.primitive_count == 0
        || header.primitive_count as u128 * 23 > misaka_palw_kernel::flat_scope9::MAX_SCOPE_RELATION_VISITS_V9
        || bytes.len() != at
    {
        return Err("native class exact fixed header/complete-scope count differs".into());
    }
    Ok((header, at))
}

/// Internally produced immutable exact-job preparation. Its expensive actual
/// resource derivation is paid in a separate preparation route. Funding can
/// restore this authenticated fixed summary without repeating that walk.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeExecutionPreparationV11 {
    pub record: misaka_palw_kernel::flat_execution11::NativeExecutionPreparationRecordV11,
    pub plan: QuotaPlanRecordV11,
    pub owner: PalwBondKeyV2,
    pub prepared_daa: u64,
}
impl NativeExecutionPreparationV11 {
    pub fn id(&self) -> Digest {
        // Time/bond rotation do not turn the same independently derived exact
        // class/input/plan into new quantities or a new job credit.
        self.record.root()
    }
    pub fn checked_plan(&self, class: &ExecutionClassRecordV11) -> Result<CheckedJobQuotaPlanV11, String> {
        if self.record.version != 11
            || self.record.class_id != class.id()
            || self.record.input.version != 11
            || self.plan.program_root != class.program_root
            || self.plan.parameter_root != class.parameter_root
            || self.plan.class_scope_root != class.class_scope9_root
            || self.plan.positions != self.record.input.positions
            || self.plan.root() != self.record.quota_plan_root
        {
            return Err("native exact preparation differs from original class/input".into());
        }
        CheckedJobQuotaPlanV11::restore(&self.plan, &self.plan.root())
    }
}

/// Exact original funded book plus its requester operator and fixed payout.
/// Both remain available through all proof/DA retention and quota collection;
/// the monotone operator counter remains afterwards to prevent another Fund.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeExecutionJobHeaderV11 {
    pub version: u16,
    pub job_id: Digest,
    pub class_id: Digest,
    pub original_class_header_root: Digest,
    pub funded_job_root: Digest,
    pub book_root: Digest,
    pub quota_plan_root: Digest,
    pub requester: PalwBondKeyV2,
    pub requester_operator: Digest,
    pub requester_payout: Hash64,
    pub producer: PalwBondKeyV2,
    pub producer_operator: Digest,
    pub producer_payout: Hash64,
    pub book_bytes_len: u32,
    pub prompt_len: u32,
    pub stored_post_cap: u64,
    pub deadline_daa: u64,
    pub terminal: misaka_palw_kernel::flat_execution11::ExecutionTerminalV11,
    pub producer_reserved: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct NativeExecutionJobRowV11 {
    pub header: NativeExecutionJobHeaderV11,
    pub book_bytes: Vec<u8>,
    pub quota_plan: QuotaPlanRecordV11,
    pub prompt: Vec<u32>,
}
impl NativeExecutionJobRowV11 {
    pub fn restore_book<C: misaka_palw_kernel::flat_execution11::ExecutionClassContextV11>(
        &self,
        class: &C,
    ) -> Result<ExecutionJobStateV11, String> {
        let book = ExecutionJobStateV11::from_bytes(&self.book_bytes, class, self.header.funded_job_root, self.header.book_root)?;
        let job = book.job();
        if self.header.version != 11
            || self.header.job_id != job.id()
            || self.header.class_id != job.class_id
            || self.header.quota_plan_root != job.quota_plan_root
            || self.header.deadline_daa != job.deadline_daa
            || self.header.prompt_len != job.input.prompt_len
            || self.prompt.len() != self.header.prompt_len as usize
            || misaka_palw_kernel::seg::prompt_root_of_ids_v1(&self.prompt) != job.input.prompt_root
            || crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&self.header.requester) != job.requester
            || crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&self.header.producer) != job.producer
            || self.header.book_bytes_len as usize != self.book_bytes.len()
            || self.header.terminal != book.terminal()
            || self.header.producer_reserved
                != if book.terminal() == misaka_palw_kernel::flat_execution11::ExecutionTerminalV11::Challengeable {
                    book.claim().ok_or("challengeable native book lacks its claim")?.reserved_original
                } else {
                    0
                }
        {
            return Err("original native funded job/header/prompt differs".into());
        }
        let plan = CheckedJobQuotaPlanV11::restore(&self.quota_plan, &job.quota_plan_root)?;
        if plan.class_scope_root() != class.record().class_scope9_root
            || plan.resource_root() != job.actual_resource10_root
            || plan.actual_scope_root() != job.actual_scope9_root
            || self.header.stored_post_cap != plan.native_post_carrier_cap()
        {
            return Err("original native exact quota plan differs from funded job".into());
        }
        Ok(book)
    }
    pub fn replace_book(&mut self, book: &ExecutionJobStateV11) -> Result<(), String> {
        if book.job().root() != self.header.funded_job_root || book.job().id() != self.header.job_id {
            return Err("native book replacement changed original funding".into());
        }
        self.book_bytes = borsh::to_vec(book).map_err(|e| e.to_string())?;
        self.header.book_bytes_len = u32::try_from(self.book_bytes.len()).map_err(|_| "native book size overflow")?;
        self.header.book_root = book.root();
        self.header.terminal = book.terminal();
        self.header.producer_reserved = if book.terminal() == misaka_palw_kernel::flat_execution11::ExecutionTerminalV11::Challengeable
        {
            book.claim().ok_or("challengeable native book lacks original claim")?.reserved_original
        } else {
            0
        };
        Ok(())
    }
}

/// Read a fixed header and check the complete raw row ceiling before any
/// book/prompt Vec allocation. This reader accepts prior native journal rows,
/// not price declarations from a funding carrier or remote import.
pub fn native_execution_job_header_v11(bytes: &[u8]) -> Result<NativeExecutionJobHeaderV11, String> {
    let mut r = Cursor::new(bytes);
    let header = NativeExecutionJobHeaderV11::deserialize_reader(&mut r).map_err(|_| "invalid fixed original funded job header")?;
    let book_len = u32::deserialize_reader(&mut r).map_err(|_| "truncated original funded book frame")?;
    if header.version != 11
        || book_len != header.book_bytes_len
        || book_len as usize > misaka_palw_kernel::flat_execution11::EXECUTION_STATE_MAX_BYTES_V11
        || header.prompt_len == 0
        || header.prompt_len as usize > FLAT_MODEL_VECTOR_MAX_POST_BYTES_V3 / 4
        || header.stored_post_cap > FLAT_MODEL_VECTOR_MAX_POST_BYTES_V3 as u64
    {
        return Err("invalid bounded original funded job frame".into());
    }
    let max = (r.position() as usize)
        .checked_add(book_len as usize)
        .and_then(|n| n.checked_add(4096))
        .and_then(|n| n.checked_add(4))
        .and_then(|n| n.checked_add(header.prompt_len as usize * 4))
        .ok_or("original funded job raw bound overflow")?;
    if bytes.len() > max || bytes.len().saturating_sub(r.position() as usize) < book_len as usize {
        return Err("original funded job exceeds pre-copy raw bound".into());
    }
    Ok(header)
}
pub fn decode_native_execution_job_v11(bytes: &[u8]) -> Result<NativeExecutionJobRowV11, String> {
    let header = native_execution_job_header_v11(bytes)?;
    let mut r = Cursor::new(bytes);
    let decoded = NativeExecutionJobHeaderV11::deserialize_reader(&mut r).map_err(|_| "native job header decode")?;
    let n = u32::deserialize_reader(&mut r).map_err(|_| "native book framing")? as usize;
    let begin = r.position() as usize;
    let end = begin.checked_add(n).filter(|n| *n <= bytes.len()).ok_or("native book frame overflow")?;
    let book_bytes = bytes[begin..end].to_vec();
    r.set_position(end as u64);
    let quota_plan = QuotaPlanRecordV11::deserialize_reader(&mut r).map_err(|_| "invalid fixed original quota plan")?;
    let count = u32::deserialize_reader(&mut r).map_err(|_| "native prompt framing")?;
    if count != header.prompt_len
        || count as usize > bytes.len().saturating_sub(r.position() as usize) / 4
        || bytes.len().saturating_sub(r.position() as usize) != count as usize * 4
    {
        return Err("native funded prompt count/trailing bytes differ".into());
    }
    let mut prompt = Vec::with_capacity(count as usize);
    for _ in 0..count {
        prompt.push(u32::deserialize_reader(&mut r).map_err(|_| "truncated native funded prompt")?);
    }
    Ok(NativeExecutionJobRowV11 { header: decoded, book_bytes, quota_plan, prompt })
}

impl PalwKernelRouteStateV1 {
    pub fn execution_class_header_v11(&self, id: &Digest) -> Option<NativeExecutionClassJournalV11> {
        let (header, _) =
            native_execution_class_header_v11(self.aux.get(&(EXECUTION_CLASS_TABLE_V11, borsh::to_vec(id).ok()?))?).ok()?;
        (header.record.id() == *id).then_some(header)
    }
    pub fn execution_job_header_v11(&self, id: &Digest) -> Option<NativeExecutionJobHeaderV11> {
        let header = native_execution_job_header_v11(self.aux.get(&(EXECUTION_JOB_TABLE_V11, borsh::to_vec(id).ok()?))?).ok()?;
        (header.job_id == *id).then_some(header)
    }
    pub fn execution_job_row_v11(&self, id: &Digest) -> Result<NativeExecutionJobRowV11, String> {
        let row = decode_native_execution_job_v11(
            self.aux
                .get(&(EXECUTION_JOB_TABLE_V11, borsh::to_vec(id).map_err(|e| e.to_string())?))
                .ok_or("unknown native funded execution job")?,
        )?;
        if row.header.job_id != *id {
            return Err("native funded job namespace differs".into());
        }
        Ok(row)
    }
    pub fn execution_nonce_counter_v11(&self, operator: &Digest) -> Result<NativeRequesterNonceRowV11, String> {
        let key = borsh::to_vec(operator).map_err(|e| e.to_string())?;
        let Some(bytes) = self.aux.get(&(EXECUTION_REQUEST_INDEX_TABLE_V11, key)) else {
            return Ok(NativeRequesterNonceRowV11 { operator: *operator, next_nonce: 0 });
        };
        if bytes.len() != 72 {
            return Err("native requester counter is not a fixed rooted row".into());
        }
        let row: NativeRequesterNonceRowV11 = borsh::from_slice(bytes).map_err(|_| "invalid native requester counter")?;
        if row.operator != *operator {
            return Err("native requester counter namespace differs".into());
        }
        Ok(row)
    }
}

/// The native class was independently admitted and is restored under a paid
/// original journal root. Reuse that complete checked scope on a job action;
/// do not schedule another hidden whole-scope/resource walk. This helper grants
/// no posting, coin or Final entitlement without the original funded book.
pub fn checked_flat_execution_post_v11(
    route: &PalwKernelRouteStateV1,
    class: &misaka_palw_kernel::flat_execution11::CheckedNativeExecutionClassV11,
    book: &ExecutionJobStateV11,
    post: &FlatExecutionPostV11,
) -> Result<crate::palw_flat_model_vector_v9::FlatModelVectorViewV9, String> {
    use misaka_palw_kernel::{descriptor::*, evidence::*, seg::*, seg_ledger::*};
    let c = class.record();
    let j = book.job();
    let t = &post.body.trace;
    let v = class.view();
    let descriptor = [k2_tir_v4_descriptor(), k2_tir_v5_descriptor()]
        .into_iter()
        .find(|d| d.digest() == c.descriptor)
        .ok_or("unknown execution11 text role")?;
    let evidence_header = EvidenceHeaderV1 {
        network_domain: route.header.policy.network_domain,
        ruleset_digest: route.header.policy.ruleset_digest,
        class_binding_id: c.candidate9_id,
        program_root: c.program_root,
        artifact_root: c.parameter_root,
        plan_root: v.plan.root(),
    };
    if post.version != 11
        || post.execution_class != c.id()
        || post.job_id != j.id()
        || j.class_id != c.id()
        || post.requester_nonce != j.nonce
        || post.job_input_root != j.input.root()
        || post.actual_scope9_root != j.actual_scope9_root
        || post.actual_resource10_root != j.actual_resource10_root
        || post.quota_plan_root != j.quota_plan_root
        || post.body.version != 9
        || post.body.profile != v.scope.profile
        || post.body.court_set != v.scope.court_set
        || post.body.scope_root != c.class_scope9_root
        || post.body.replay_root != c.replay9_root
        || post.body.resource_root != c.resource10_root
        || t.version != 2
        || t.evidence.header != evidence_header
        || t.evidence.suite != SuiteParamsV1::of(&descriptor)
        || t.evidence.positions != j.input.positions
        || t.prompt.len() != j.input.prompt_len as usize
        || prompt_root_of_ids_v1(&t.prompt) != j.input.prompt_root
        || t.max_new_tokens != j.input.generated_count
        || t.generated.len() != j.input.generated_count as usize
        || t.prompt.iter().chain(&t.generated).any(|x| *x >= v.program.token_bound)
        || t.implementation_roots != [post.body.trace_root(c.candidate9_id, c.binding_id); 3]
    {
        return Err("funded execution11 outer job identity or exact typed trace differs".into());
    }
    t.evidence.check_structure(&t.segment_roots)?;
    let fed = if let Some(encoder) = v.encoder {
        if t.prompt.is_empty() || t.prompt.len() > encoder.l as usize || !t.generated.is_empty() || t.evidence.positions != 1 {
            return Err("funded execution11 encoder input differs".into());
        }
        &[][..]
    } else {
        if t.generated.is_empty()
            || t.prompt.len().checked_add(t.generated.len()).and_then(|n| n.checked_sub(1)) != Some(t.evidence.positions as usize)
        {
            return Err("funded execution11 decoder must deliver its exact fixed count".into());
        }
        &t.generated[..t.generated.len() - 1]
    };
    if t.evidence.job_input_root != job_input_root_v2(j.input.prompt_len, &j.input.prompt_root, fed) {
        return Err("funded execution11 fed-back input root differs".into());
    }
    Ok(crate::palw_flat_model_vector_v9::FlatModelVectorViewV9 {
        profile: post.body.profile,
        scope: v.scope.clone(),
        replay: v.replay.clone(),
        trace: SegClaimViewV1 {
            program: v.program.clone(),
            params: class.parameters().clone(),
            segment_roots: t.segment_roots.clone(),
            positions: t.evidence.positions,
            job: JobViewV1 {
                class_binding_id: c.candidate9_id,
                prompt_len: j.input.prompt_len,
                prompt_root: j.input.prompt_root,
                inline_prompt: Some(t.prompt.clone()),
                max_new_tokens: j.input.generated_count,
                decode: misaka_palw_kernel::job::DecodeRuleV1::Greedy,
                complete: true,
            },
            generated: t.generated.clone(),
            encoder: v.encoder,
        },
    })
}

impl FlatExecutionPostV11 {
    /// The segmented contract has no standalone final state tensor root. Keep
    /// its authenticated trace/state view and token delivery separately named;
    /// neither digest claims an empirical source-model or software certificate.
    pub fn native_trace(&self, class: &ExecutionClassRecordV11) -> misaka_palw_kernel::flat_execution11::NativeProtectedTraceV11 {
        let evidence = &self.body.trace.evidence;
        misaka_palw_kernel::flat_execution11::NativeProtectedTraceV11 {
            vector: self.id(class.candidate9_id, class.binding_id).as_bytes(),
            candidate9_id: class.candidate9_id,
            binding_id: class.binding_id,
            class_scope9_root: class.class_scope9_root,
            replay9_root: class.replay9_root,
            class_resource10_root: class.resource10_root,
            prompt_root: misaka_palw_kernel::seg::prompt_root_of_ids_v1(&self.body.trace.prompt),
            prompt_len: self.body.trace.prompt.len() as u32,
            positions: evidence.positions,
            generated_count: self.body.trace.generated.len() as u32,
            trace_root: self.trace_root(class.candidate9_id, class.binding_id),
            output_root: object_id(
                b"misaka-palw/native/funded-delivery/v11",
                &(class.encoder, &self.body.trace.generated, evidence.claim_root),
            ),
            final_state_root: object_id(
                b"misaka-palw/native/funded-retained-trace-state-view/v11",
                &(evidence.positions, evidence.claim_root, self.body.trace.segment_roots.last().copied()),
            ),
        }
    }
}

/// Immutable network configuration, never accepted from a transaction body.
/// Its identity covers producer lifetime deliveries as well as original money
/// and all 21 quota axes. A custom devnet configuration can exercise mechanics;
/// this tuple does not certify measured population/model coverage.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeExecutionPoliciesV11 {
    pub execution: misaka_palw_kernel::flat_execution11::ExecutionPolicyV11,
    pub quota: misaka_palw_kernel::flat_quota11::QuotaPolicyV11,
    pub discovery: misaka_palw_kernel::flat_quota11::discovery13::DiscoveryPolicyV13,
}
impl NativeExecutionPoliciesV11 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/funded-execution-policies/v11", self)
    }
    pub fn check(&self, network_domain: Digest) -> Result<(), String> {
        self.execution.check(&self.quota)?;
        self.discovery.check(&self.quota)?;
        if self.execution.network_domain != network_domain || self.quota.network_domain != network_domain {
            return Err("funded native execution policy belongs to another network".into());
        }
        Ok(())
    }
}

pub const EXECUTION_ACTION_REGISTER_V11: u8 = 0;
pub const EXECUTION_ACTION_PREPARE_V11: u8 = 1;
pub const EXECUTION_ACTION_FUND_V11: u8 = 2;
pub const EXECUTION_ACTION_COMMIT_V11: u8 = 3;
pub const EXECUTION_ACTION_CLOSE_V11: u8 = 4;
pub const EXECUTION_ACTION_SEAL_V11: u8 = 5;
pub const EXECUTION_ACTION_REVEAL_V11: u8 = 6;
pub const EXECUTION_ACTION_EXPIRE_V11: u8 = 7;
pub const EXECUTION_ACTION_DEMAND_V11: u8 = 8;
pub const EXECUTION_ACTION_ANSWER_V11: u8 = 9;
pub const EXECUTION_ACTION_SETTLE_DEMAND_V11: u8 = 10;
pub const EXECUTION_ACTION_DISCOVER_V11: u8 = 11;
pub const EXECUTION_ACTION_CONVERT_V11: u8 = 12;

/// Native close state, derived from authenticated rows. Counters distinguish
/// paid discovery from ordinary requests and preserve lifetime answer bounds.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeExecutionPursuitV11 {
    pub ordinary_requests: u32,
    pub open_demands: u32,
    pub retained_seal_keys: Vec<Digest>,
    pub retained_discovery_keys: Vec<Digest>,
    pub latest_answer_proof_until: u64,
    pub required_retained_until: u64,
}
/// Separate fixed counters preserve the v11 pursuit framing. Ordinary callers
/// may occupy three units; the four original protected tickets retain two each.
/// The monotone ordinal indexes bounded cleanup without scanning the DA table.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeExecutionDemandCountersV13 {
    pub open_ordinary: u32,
    pub next_ordinal: u32,
    pub collected_ordinal: u32,
}
pub const NATIVE_EXECUTION_OPEN_ORDINARY_V13: u32 = 3;
pub const NATIVE_EXECUTION_OPEN_TOTAL_V13: u32 = NATIVE_EXECUTION_OPEN_ORDINARY_V13 + 4 * 2;
impl NativeExecutionPursuitV11 {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        // At most four retained shared seal/discovery tickets. Inspect both
        // vector counts before any allocation, independently of rooted state.
        if bytes.len() < 32 || bytes.len() > 32 + 4 * 64 {
            return Err("native pursuit fixed extent".into());
        }
        let a = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let b_at = 12usize.checked_add(a.checked_mul(64).ok_or("pursuit count overflow")?).ok_or("pursuit count overflow")?;
        let b = u32::from_le_bytes(bytes.get(b_at..b_at + 4).ok_or("truncated native pursuit")?.try_into().unwrap()) as usize;
        if a + b > 4 || bytes.len() != 32 + (a + b) * 64 {
            return Err("native pursuit shared retained count".into());
        }
        borsh::from_slice(bytes).map_err(|_| "native pursuit framing".into())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeExecutionDemandRequestV11 {
    pub job_id: Digest,
    pub slot: u8,
    pub position: u32,
    pub occurrence: u16,
    pub node: u16,
    /// 0 node, 1 hash, 2 selected value, 3 typed header.
    pub kind: u8,
    pub index: u64,
    /// 0 ordinary; 1 original paid own proof; 2 original paid discovery.
    pub lane: u8,
    pub entitlement: Digest,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeExecutionDemandV11 {
    pub job_id: Digest,
    pub lane: u8,
    pub entitlement: Digest,
    pub body: crate::palw_flat_model_vector_da_v9::FlatVectorDemandV9,
}
impl NativeExecutionDemandV11 {
    pub fn id(&self) -> Hash64 {
        self.body.id()
    }
}

impl PalwKernelRouteStateV1 {
    /// Readonly original rooted execution, also used by public scoped replay.
    /// This never grants a fresh Fund/Commit or trusts a carrier's verification.
    pub fn execution_view_v11(&self, id: &Digest) -> Result<crate::palw_flat_model_vector_v9::FlatModelVectorViewV9, String> {
        use misaka_palw_kernel::flat_execution11::*;
        let row = self.execution_job_row_v11(id)?;
        let header = self.execution_class_header_v11(&row.header.class_id).ok_or("native original class absent")?;
        let fixed = header.checked_fixed_header()?;
        let book = row.restore_book(&fixed)?;
        let c = &header.record;
        let binding =
            self.model_artifact_binding_header_v2(&Hash64::from_bytes(c.binding_id)).ok_or("native original source absent")?;
        let source = NativeSourceBindingV11 {
            descriptor: binding.descriptor,
            program_root: binding.program_root,
            parameter_root: binding.kernel_param_root.as_bytes(),
            inventory_root: binding.model_inventory_root.as_bytes(),
            bound_daa: binding.bound_daa,
            matures_daa: binding.matures_daa,
            refuted: binding.refuted,
        };
        let candidate =
            self.flat_candidate_record_v9(&Hash64::from_bytes(c.candidate9_id)).ok_or("native original candidate absent")?;
        let policies: NativeExecutionPoliciesV11 = self.aux_row(EXECUTION_POLICY_TABLE_V11, &[]).ok_or("native policies absent")?;
        let class = restore_native_execution_class_header_v11(
            &candidate,
            &source,
            Some(c),
            &policies.execution,
            &policies.quota,
            self.header.scalars.daa,
        )?;
        let raw = self
            .aux
            .get(&(EXECUTION_PROTECTED_TABLE_V11, borsh::to_vec(id).map_err(|e| e.to_string())?))
            .ok_or("native original post absent")?;
        if raw.len() as u64 > row.header.stored_post_cap {
            return Err("native original post ceiling".into());
        }
        let post = decode_flat_execution_post_v11(raw).map_err(|e| e.to_string())?;
        checked_flat_execution_post_v11(self, &class, &book, &post)
    }
    pub fn execution_demand_v11(&self, id: &Hash64) -> Option<NativeExecutionDemandV11> {
        let raw = self.aux.get(&(EXECUTION_DEMAND_TABLE_V11, borsh::to_vec(id).ok()?))?;
        if raw.len() > 2048 {
            return None;
        }
        let row: NativeExecutionDemandV11 = borsh::from_slice(raw).ok()?;
        (row.id() == *id).then_some(row)
    }
}

/// Original pre-proof native debit and nonce. Converted/retired status never
/// changes the immutable projection that authorized this discovery ticket.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeExecutionDiscoveryV13 {
    pub job_id: Digest,
    pub funded_job_root: Digest,
    pub book_root: Digest,
    pub claim_id: Digest,
    pub vector: Digest,
    pub actor: Digest,
    pub operator: Digest,
    pub producer_operator: Digest,
    pub nonce: Digest,
    pub posted_daa: u64,
    pub matures_daa: u64,
    pub horizon_daa: u64,
    pub original_debit: u64,
    pub original_debit_receipt: Digest,
    pub original_paid_root: Digest,
    pub converted_to: Option<Digest>,
}
impl NativeExecutionDiscoveryV13 {
    pub fn native(&self) -> misaka_palw_kernel::flat_quota11::discovery13::NativePaidDiscoveryV13 {
        use misaka_palw_kernel::flat_quota11::discovery13::NativePaidDiscoveryV13;
        NativePaidDiscoveryV13 {
            job_id: self.job_id,
            funded_job_root: self.funded_job_root,
            book_root: self.book_root,
            claim_id: self.claim_id,
            vector_id: self.vector,
            actor: self.actor,
            actor_operator: self.operator,
            producer_operator: self.producer_operator,
            nonce: self.nonce,
            posted_daa: self.posted_daa,
            matures_daa: self.matures_daa,
            horizon_daa: self.horizon_daa,
            debit: self.original_debit,
            debit_receipt: self.original_debit_receipt,
            original_root: self.original_paid_root,
        }
    }
    pub fn key(&self) -> Digest {
        self.native().key()
    }
}

/// Read the exact fixed quota record after the bounded book frame; do not copy
/// the prompt or post just to quote a native fragment/answer operation.
pub fn native_execution_job_plan_v11(bytes: &[u8]) -> Result<(NativeExecutionJobHeaderV11, QuotaPlanRecordV11), String> {
    let header = native_execution_job_header_v11(bytes)?;
    let mut r = Cursor::new(bytes);
    let _: NativeExecutionJobHeaderV11 = NativeExecutionJobHeaderV11::deserialize_reader(&mut r).map_err(|_| "native fixed header")?;
    let len = u32::deserialize_reader(&mut r).map_err(|_| "native book frame")?;
    let end = (r.position() as usize).checked_add(len as usize).filter(|e| *e <= bytes.len()).ok_or("native book framing overflow")?;
    r.set_position(end as u64);
    let plan = QuotaPlanRecordV11::deserialize_reader(&mut r).map_err(|_| "native fixed quota plan")?;
    if plan.root() != header.quota_plan_root {
        return Err("native fixed original quota plan root differs".into());
    }
    Ok((header, plan))
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeExecutionProofGroupV11 {
    pub group: Digest,
    pub seal: Digest,
    pub opener: PalwBondKeyV2,
    pub count: u8,
    pub opened_daa: u64,
    pub expires_daa: u64,
    pub parts: std::collections::BTreeMap<u8, Vec<u8>>,
}
impl NativeExecutionProofGroupV11 {
    pub fn from_bytes(bytes: &[u8], cap: usize) -> Result<Self, String> {
        let mut r = Cursor::new(bytes);
        let _: Digest = Digest::deserialize_reader(&mut r).map_err(|_| "native group id")?;
        let _: Digest = Digest::deserialize_reader(&mut r).map_err(|_| "native group paid seal")?;
        let _: PalwBondKeyV2 = PalwBondKeyV2::deserialize_reader(&mut r).map_err(|_| "native group opener")?;
        let declared = u8::deserialize_reader(&mut r).map_err(|_| "native group count")?;
        let _: u64 = u64::deserialize_reader(&mut r).map_err(|_| "native group clock")?;
        let _: u64 = u64::deserialize_reader(&mut r).map_err(|_| "native group clock")?;
        let n = u32::deserialize_reader(&mut r).map_err(|_| "native group parts count")?;
        if declared == 0 || declared > 64 || n > declared as u32 {
            return Err("native group bounded parts".into());
        }
        let mut previous = None;
        let mut total = 0usize;
        for _ in 0..n {
            let i = u8::deserialize_reader(&mut r).map_err(|_| "native group part index")?;
            let size = u32::deserialize_reader(&mut r).map_err(|_| "native group part count")? as usize;
            if i >= declared
                || previous.is_some_and(|p| p >= i)
                || size == 0
                || size > misaka_palw_kernel::flat_quota11::PROTECTED_FRAGMENT_BYTES_V11 as usize
            {
                return Err("native group index/body bounds".into());
            }
            total = total.checked_add(size).filter(|n| *n <= cap).ok_or("native group aggregate cap")?;
            let end = (r.position() as usize).checked_add(size).filter(|n| *n <= bytes.len()).ok_or("native group truncated part")?;
            r.set_position(end as u64);
            previous = Some(i);
        }
        if r.position() as usize != bytes.len() {
            return Err("native group trailing bytes".into());
        }
        borsh::from_slice(bytes).map_err(|_| "native group canonical framing".into())
    }
}
impl PalwKernelRouteStateV1 {
    pub fn execution_seal_v11(&self, id: &Digest) -> Option<NativeExecutionSealV11> {
        let raw = self.aux.get(&(EXECUTION_SEAL_TABLE_V11, borsh::to_vec(id).ok()?))?;
        if raw.len() > 2048 {
            return None;
        }
        let s: NativeExecutionSealV11 = borsh::from_slice(raw).ok()?;
        let k = misaka_palw_kernel::flat_quota11::ProtectedSealKeyV11 {
            job_id: s.job_id,
            vector_id: s.vector,
            seal_actor: s.actor,
            proof_root: s.proof_root,
            horizon_daa: s.horizon_daa,
        };
        (s.checked_original_paid_root().is_ok()
            && s.version == 11
            && k.root() == *id
            && crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&s.bond) == s.actor)
            .then_some(s)
    }
    pub fn funded_proof_chunk_cap_v11(&self, id: &Digest) -> Option<usize> {
        let s = self.execution_seal_v11(id)?;
        let raw = self.aux.get(&(EXECUTION_JOB_TABLE_V11, borsh::to_vec(&s.job_id).ok()?))?;
        let (_, record) = native_execution_job_plan_v11(raw).ok()?;
        let plan = CheckedJobQuotaPlanV11::restore(&record, &record.root()).ok()?;
        usize::try_from(misaka_palw_kernel::flat_quota11::derive_protected_seal_plan_v11(&plan).ok()?.assembly_cap).ok()
    }
    pub fn funded_proof_group_v11(&self, id: &Digest) -> Option<NativeExecutionProofGroupV11> {
        let cap = self.funded_proof_chunk_cap_v11(id)?;
        let group = NativeExecutionProofGroupV11::from_bytes(
            self.aux.get(&(EXECUTION_SEAL_TABLE_V11, borsh::to_vec(&(12u8, *id)).ok()?))?,
            cap,
        )
        .ok()?;
        (group.seal == *id && group.parts.is_empty()).then_some(group)
    }
    pub fn funded_proof_chunk_completion_v11(
        &self,
        chunk: &crate::palw_kernel_route_v1::PalwKernelChunkV1,
    ) -> Option<crate::palw_state_v2::PalwConsensusObjectV2> {
        use crate::palw_kernel_route_v1::PalwKernelChunkTargetV1;
        let PalwKernelChunkTargetV1::FundedProofV11 { seal, actor } = chunk.target else { return None };
        let original = self.execution_seal_v11(&seal)?;
        if actor != original.actor
            || chunk.opener != original.bond
            || original.spent
            || chunk.count == 0
            || chunk.count > 64
            || chunk.index >= chunk.count
            || chunk.bytes.is_empty()
            || chunk.bytes.len() > misaka_palw_kernel::flat_quota11::PROTECTED_FRAGMENT_BYTES_V11 as usize
        {
            return None;
        }
        let cap = self.funded_proof_chunk_cap_v11(&seal)?;
        let stored = self.funded_proof_group_v11(&seal);
        let mut parts: std::collections::BTreeMap<u8, &[u8]> = Default::default();
        if let Some(g) = &stored {
            if g.group != chunk.group.as_bytes()
                || g.opener != chunk.opener
                || g.count != chunk.count
                || g.parts.contains_key(&chunk.index)
            {
                return None;
            }
            for i in 0..g.count {
                if i == chunk.index {
                    continue;
                }
                if let Some(raw) = self.aux.get(&(EXECUTION_SEAL_TABLE_V11, borsh::to_vec(&(15u8, seal, i)).ok()?)) {
                    if raw.is_empty() || raw.len() > misaka_palw_kernel::flat_quota11::PROTECTED_FRAGMENT_BYTES_V11 as usize {
                        return None;
                    }
                    parts.insert(i, raw);
                }
            }
        }
        parts.insert(chunk.index, &chunk.bytes);
        if parts.len() != chunk.count as usize {
            return None;
        }
        let sum = parts.values().try_fold(0usize, |n, b| n.checked_add(b.len()))?;
        if sum > cap {
            return None;
        }
        let mut bytes = Vec::with_capacity(sum);
        for i in 0..chunk.count {
            bytes.extend_from_slice(parts.get(&i)?);
        }
        if crate::palw_state_v2::palw_object_chunk_group_id_v1(&bytes) != chunk.group {
            return None;
        }
        let inner = borsh::from_slice(&bytes).ok()?;
        crate::palw_kernel_route_v1::palw_kernel_chunk_inner_matches_target_v1(&inner, &chunk.target).then_some(inner)
    }
}

pub const EXECUTION_ACTION_ACCEPT_V13: u8 = 13;
pub const EXECUTION_ACTION_ACCEPT_V18: u8 = 18;
pub const EXECUTION_ACTION_SOURCE_REQUEST_V16: u8 = 14;
pub const EXECUTION_ACTION_SOURCE_ANSWER_V16: u8 = 15;
pub const EXECUTION_ACTION_SOURCE_SETTLE_V16: u8 = 16;
pub const EXECUTION_ACTION_ACCEPT_V23: u8 = 23;
pub const EXECUTION_ACTION_FUND_V23: u8 = 24;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeSourceRequestV16 {
    pub job: Digest,
    pub lane: u8,
    pub entitlement: Digest,
    pub slot: u8,
    pub param: u16,
    /// 0 means None; 1..65536 identify the real original layer 0..65535.
    pub layer_code: u32,
    pub kind: u8,
    pub axis: u8,
    pub level: u8,
    pub index: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeSourceDemandV16 {
    pub request: NativeSourceRequestV16,
    pub requester: PalwBondKeyV2,
    pub requester_payout: Hash64,
    pub producer: PalwBondKeyV2,
    pub producer_payout: Hash64,
    pub pc: Digest,
    pub declared_dtype: u8,
    pub declared_rank: u8,
    pub declared_shape: [u64; 4],
    pub filed: u64,
    pub deadline: u64,
    pub proof_until: u64,
    pub bond: u64,
    pub attempts: u8,
    pub settled: bool,
}
impl NativeSourceDemandV16 {
    pub fn id(&self) -> Digest {
        object_id(
            b"misaka-palw/native/original-bound-pc-demand/v16",
            &(&self.request, self.requester, self.producer, self.pc, self.filed, self.deadline, self.bond),
        )
    }
}
/// Producer authorization precedes any reservation against its actor/resource
/// or monetary windows. The original request carrier retains its frozen bytes.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeExecutionConsentV13 {
    pub version: u16,
    pub preparation: Digest,
    pub class_id: Digest,
    pub original_class_header_root: Digest,
    pub policies_root: Digest,
    pub input_root: Digest,
    pub quota_plan_root: Digest,
    pub requester: PalwBondKeyV2,
    pub requester_operator: Digest,
    pub producer: PalwBondKeyV2,
    pub producer_operator: Digest,
    pub nonce: Digest,
    pub reward: u64,
    pub accepted_daa: u64,
    pub expires_daa: u64,
    pub network_interval: u64,
}
impl NativeExecutionConsentV13 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-producer-consent/v13", self)
    }
}
/// Separate contract commits additional PC availability and trace restoration
/// reservations. A v13 signature alone cannot authorize these extra charges.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeExecutionConsentV18 {
    pub version: u16,
    pub original_consent13_root: Digest,
    pub source_plan16_root: Digest,
    pub trace_plan17_root: Digest,
    pub availability_terms_root: Digest,
    pub fee_plan19_root: Digest,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeExecutionFeePlanV19 {
    pub version: u16,
    pub class: Digest,
    pub quota_plan: Digest,
    pub original_consent13: Digest,
    pub reward: u64,
    pub maximum: u64,
    pub trace_units: u32,
    pub source_units: u32,
    pub trace_unit_fee: u64,
    pub source_unit_fee: u64,
}
impl NativeExecutionFeePlanV19 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-delivery-fee-plan/v19", self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeExecutionFeesV19 {
    pub version: u16,
    pub job: Digest,
    pub funded_job_root: Digest,
    pub original_consent18: Digest,
    pub plan: NativeExecutionFeePlanV19,
    pub earned: u64,
}
impl NativeExecutionConsentV18 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-producer-consent/v18", self)
    }
}
