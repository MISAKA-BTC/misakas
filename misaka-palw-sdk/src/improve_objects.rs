//! **RFC-0004's objects, built and signed offline (work item A10)**: what `palw-class improve …` writes
//! for `misaka palw submit-object` to carry — a governed line's policy (tag 70), hard cases (71), a
//! job's data-use opt-in (72), a bonded steward's setter set (73) and its two reveals (74, 75),
//! registered datasets (76), teaching artifacts (77, 78), a teacher licence (79), a candidate (80) and a
//! rollback (81). Each is the chain's own payload type with every derived id computed by the consensus
//! function that checks it (never typed), signed under the chain's message and context
//! (`palw_improvement_policy_message_v1`, `palw_improve_material_message_v1`,
//! `palw_candidate_submission_message_v1`, `palw_improvement_rollback_message_v1`), so an object this
//! module builds is the object the acceptance layer verifies.
//!
//! **Why offline.** Like `misaka palw tir-registration` and `palw-class certify`, building an object needs
//! the network's domain and a key, not a node: the carrier that funds and submits it is
//! `misaka palw submit-object` (a wallet's work). A reveal (74, 75, 78) is unsigned — it opens a
//! commitment only its maker's salt opens — and is written beside its commitment's object, ready for the
//! window in which the chain takes it.
//!
//! The JSON specs the CLI reads are in [`spec`]: ids are 128 lowercase hex characters, token ids are
//! arrays of integers, a salt is hex or drawn at random (and then written beside the object, because a
//! reveal needs it).

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_improve_artifact_v1::PalwTirArtifactRefV1;
use kaspa_consensus_core::palw_improve_candidate_v1::{
    PALW_IMPROVE_CANDIDATE_MLDSA87_CONTEXT_V1, PalwCandidateDeclarationsV1, PalwCandidateSubmissionV1,
    palw_candidate_submission_message_v1,
};
use kaspa_consensus_core::palw_improve_material_v1::{
    PALW_IMPROVE_MATERIAL_MLDSA87_CONTEXT_V1, PalwCaseReferenceV1, PalwCaseSourceV1, PalwDataUseOptInV1, PalwDatasetV1,
    PalwHardCaseV1, PalwSetterKeysRevealV1, PalwSetterSetCommitmentV1, PalwSetterSetRevealV1, PalwTeacherLicenceV1,
    PalwTeachingArtifactCommitV1, PalwTeachingArtifactV1, palw_case_key_commitment_v1, palw_dataset_id_v1, palw_hard_case_id_v1,
    palw_improve_material_message_v1, palw_setter_keys_commitment_v1, palw_setter_prompts_commitment_v1, palw_setter_set_id_v1,
    palw_teacher_licence_id_v1, palw_teaching_artifact_commit_v1,
};
use kaspa_consensus_core::palw_improve_policy_v1::{
    PALW_IMPROVE_POLICY_MLDSA87_CONTEXT, PALW_IMPROVE_ROLLBACK_MLDSA87_CONTEXT, palw_improvement_policy_message_v1,
    palw_improvement_rollback_message_v1,
};
use kaspa_consensus_core::palw_improve_state_v1::{PalwImprovementPolicyV1, PalwLineageRollbackV1};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use kaspa_pq_validator_core::ValidatorKey;

/// **The signer of an improvement object**: a key and the network's domain (`palw_network_domain_v2_for`
/// of the network and its genesis — a drill's, with its salt).
pub struct PalwImproveSignerV1<'a> {
    pub key: &'a ValidatorKey,
    pub network_domain: Hash64,
}

impl PalwImproveSignerV1<'_> {
    fn sign(&self, message: Hash64, context: &[u8]) -> Vec<u8> {
        self.key.sign_with_context(message.as_byte_slice(), context).to_vec()
    }

    /// **Tag 70**: the line's owner opts in (`Some(policy)`, sequence 1), changes the policy between epochs
    /// (`Some`, the next sequence) or opts out (`None`).
    pub fn policy(&self, line_id: Hash64, sequence: u64, policy: Option<PalwImprovementPolicyV1>) -> PalwConsensusObjectV2 {
        let message = palw_improvement_policy_message_v1(self.network_domain, &line_id, sequence, policy.as_ref());
        PalwConsensusObjectV2::ModelLineImprovementPolicySet {
            payload: Box::new(kaspa_consensus_core::palw_improve_state_v1::PalwImprovementPolicySetV1 { line_id, sequence, policy }),
            signature: self.sign(message, PALW_IMPROVE_POLICY_MLDSA87_CONTEXT),
        }
    }

    fn material<T: borsh::BorshSerialize>(&self, tag: u8, payload: &T, bond: Option<&PalwBondKeyV2>) -> Vec<u8> {
        self.sign(palw_improve_material_message_v1(tag, &self.network_domain, payload, bond), PALW_IMPROVE_MATERIAL_MLDSA87_CONTEXT_V1)
    }

    /// **Tag 71**: a hard case, signed by its bonded submitter.
    pub fn hard_case(&self, submitter: PalwBondKeyV2, payload: PalwHardCaseV1) -> PalwConsensusObjectV2 {
        let signature = self.material(71, &payload, Some(&submitter));
        PalwConsensusObjectV2::HardCaseSubmitted { payload: Box::new(payload), submitter, signature }
    }

    /// **Tag 72**: a job's prompt made usable as a hard case, signed by the job's committer — the bond of
    /// the free-prompt claim the opt-in names.
    pub fn data_use_opt_in(&self, committer: PalwBondKeyV2, payload: PalwDataUseOptInV1) -> PalwConsensusObjectV2 {
        let signature = self.material(72, &payload, Some(&committer));
        PalwConsensusObjectV2::DataUseOptIn { payload: Box::new(payload), signature }
    }

    /// **Tag 73**: a private evaluation set's commitment, signed by its bonded steward.
    pub fn setter_set(&self, setter: PalwBondKeyV2, payload: PalwSetterSetCommitmentV1) -> PalwConsensusObjectV2 {
        let signature = self.material(73, &payload, Some(&setter));
        PalwConsensusObjectV2::SetterSetCommitted { payload: Box::new(payload), setter, signature }
    }

    /// **Tag 76**: a registered dataset, signed by its bonded contributor.
    pub fn dataset(&self, registrant: PalwBondKeyV2, payload: PalwDatasetV1) -> PalwConsensusObjectV2 {
        let signature = self.material(76, &payload, Some(&registrant));
        PalwConsensusObjectV2::DatasetRegistered { payload: Box::new(payload), registrant, signature }
    }

    /// **Tag 77**: a teaching artifact's commitment `H(artifact ‖ salt)`, signed by its bonded teacher.
    pub fn teaching_artifact(&self, teacher: PalwBondKeyV2, payload: PalwTeachingArtifactCommitV1) -> PalwConsensusObjectV2 {
        let signature = self.material(77, &payload, Some(&teacher));
        PalwConsensusObjectV2::TeachingArtifactCommitted { payload: Box::new(payload), teacher, signature }
    }

    /// **Tag 79**: a rights holder's licence, signed by the key it names (`rights_holder_key`), which must
    /// be this signer's key.
    pub fn licence(&self, payload: PalwTeacherLicenceV1) -> Result<PalwConsensusObjectV2, String> {
        if payload.rights_holder_key != self.key.public_key() {
            return Err("the licence's rights_holder_key is not this key's public key".into());
        }
        let signature = self.material(79, &payload, None);
        Ok(PalwConsensusObjectV2::TeacherLicenceRegistered { payload: Box::new(payload), signature })
    }

    /// **Tag 80**: a candidate, signed by its bonded submitter under the candidate's own message.
    pub fn candidate(&self, submitter: PalwBondKeyV2, payload: PalwCandidateSubmissionV1) -> PalwConsensusObjectV2 {
        let message = palw_candidate_submission_message_v1(&self.network_domain, &payload, &submitter);
        let signature = self.sign(message, PALW_IMPROVE_CANDIDATE_MLDSA87_CONTEXT_V1);
        PalwConsensusObjectV2::CandidateSubmitted { payload: Box::new(payload), submitter, signature }
    }

    /// **Tag 94** (RFC-0001 §2.10, `palw_adapter_class_v1`): a registered composite class's listing, signed by its lister's bond
    /// under the listing's own message and context.
    pub fn adapter_class_listed(
        &self,
        lister: PalwBondKeyV2,
        payload: kaspa_consensus_core::palw_adapter_class_v1::PalwAdapterClassListingV1,
    ) -> PalwConsensusObjectV2 {
        let message = kaspa_consensus_core::palw_adapter_class_v1::palw_adapter_listing_message_v1(&self.network_domain, &payload, &lister);
        let signature = self.sign(message, kaspa_consensus_core::palw_adapter_class_v1::PALW_ADAPTER_LISTING_MLDSA87_CONTEXT_V1);
        PalwConsensusObjectV2::AdapterClassListed { payload: Box::new(payload), lister, signature }
    }

    /// **Tag 81**: a rollback of the latest promotion, signed by its filer bond (the owner within the
    /// policy's window, or any bond with a proof).
    pub fn rollback(&self, filer: PalwBondKeyV2, payload: PalwLineageRollbackV1) -> PalwConsensusObjectV2 {
        let message = palw_improvement_rollback_message_v1(self.network_domain, &payload);
        let signature = self.sign(message, PALW_IMPROVE_ROLLBACK_MLDSA87_CONTEXT);
        PalwConsensusObjectV2::LineageHeadRolledBack { payload: Box::new(payload), filer, signature }
    }
}

// ---------------------------------------------------------------------------------------------
// Payloads, with every derived id computed
// ---------------------------------------------------------------------------------------------

/// **An exact-match reference**: the committed answer span (`H(line ‖ key ‖ salt)`).
pub fn improve_exact_key_reference_v1(line_id: &Hash64, key_ids: &[u32], salt: &Hash64) -> PalwCaseReferenceV1 {
    PalwCaseReferenceV1::ExactKey { commitment: palw_case_key_commitment_v1(line_id, key_ids, salt) }
}

/// **A likelihood reference**: the committed continuation.
pub fn improve_continuation_reference_v1(line_id: &Hash64, ids: &[u32], salt: &Hash64) -> PalwCaseReferenceV1 {
    PalwCaseReferenceV1::Continuation { commitment: palw_case_key_commitment_v1(line_id, ids, salt) }
}

/// **A hard case**, its `case_id` derived (`palw_hard_case_id_v1`: never typed).
pub fn improve_hard_case_v1(
    line_id: Hash64,
    domain: u16,
    prompt_ids: Vec<u32>,
    reference: PalwCaseReferenceV1,
    source: PalwCaseSourceV1,
    head_evidence: Option<Hash64>,
) -> PalwHardCaseV1 {
    let case_id = palw_hard_case_id_v1(&line_id, domain, &prompt_ids, &reference);
    PalwHardCaseV1 { line_id, case_id, domain, prompt_ids, reference, source, head_evidence }
}

/// **A setter set with its two reveals**: the commitment (tag 73's payload, its `set_id` derived over both
/// commitments), the prompts' reveal (tag 74, for `Drawing`'s end) and the keys' reveal (tag 75, for
/// `Closing`). One `salt` seals both; `keys` has one entry per prompt (empty for a judged item).
pub struct PalwImproveSetterSetV1 {
    pub commitment: PalwSetterSetCommitmentV1,
    pub prompts: PalwSetterSetRevealV1,
    pub keys: PalwSetterKeysRevealV1,
}

pub fn improve_setter_set_v1(
    line_id: Hash64,
    epoch: u64,
    prompts: Vec<Vec<u32>>,
    keys: Vec<Vec<u32>>,
    salt: Hash64,
) -> Result<PalwImproveSetterSetV1, String> {
    if prompts.len() != keys.len() {
        return Err(format!("{} prompts and {} keys: one key per prompt (empty for a judged item)", prompts.len(), keys.len()));
    }
    let mut commitment = PalwSetterSetCommitmentV1 {
        line_id,
        epoch,
        set_id: Hash64::default(),
        items: prompts.len() as u32,
        prompts_commitment: palw_setter_prompts_commitment_v1(&line_id, epoch, &prompts, &salt),
        keys_commitment: palw_setter_keys_commitment_v1(&line_id, epoch, &keys, &salt),
    };
    commitment.set_id = palw_setter_set_id_v1(&commitment);
    let set_id = commitment.set_id;
    Ok(PalwImproveSetterSetV1 {
        commitment,
        prompts: PalwSetterSetRevealV1 { line_id, epoch, set_id, prompts, salt },
        keys: PalwSetterKeysRevealV1 { line_id, epoch, set_id, keys, salt },
    })
}

/// **A registered dataset**, its id derived.
pub fn improve_dataset_v1(
    line_id: Hash64,
    content_root: Hash64,
    items: u64,
    mut license_classes: Vec<Hash64>,
    teacher_classes: u8,
    provenance_commitment: Hash64,
) -> PalwDatasetV1 {
    license_classes.sort();
    license_classes.dedup();
    let mut d = PalwDatasetV1 {
        line_id,
        dataset_id: Hash64::default(),
        content_root,
        items,
        license_classes,
        teacher_classes,
        provenance_commitment,
    };
    d.dataset_id = palw_dataset_id_v1(&d);
    d
}

/// **A teaching artifact's commitment** (tag 77's payload) over the whole artifact, its salt inside it
/// — the reveal (tag 78) is the artifact itself.
pub fn improve_teaching_commit_v1(artifact: &PalwTeachingArtifactV1) -> PalwTeachingArtifactCommitV1 {
    PalwTeachingArtifactCommitV1 { line_id: artifact.line_id, commit: palw_teaching_artifact_commit_v1(artifact) }
}

/// **A teacher licence**, its id derived.
pub fn improve_licence_v1(
    rights_holder_key: Vec<u8>,
    model_family: Hash64,
    mut domains: Vec<u16>,
    uses: u8,
    per_use_fee: u64,
    expiry_daa: u64,
) -> PalwTeacherLicenceV1 {
    domains.sort_unstable();
    domains.dedup();
    let mut l = PalwTeacherLicenceV1 {
        licence_id: Hash64::default(),
        rights_holder_key,
        model_family,
        domains,
        uses,
        per_use_fee,
        expiry_daa,
    };
    l.licence_id = palw_teacher_licence_id_v1(&l);
    l
}

/// **A candidate's submission** over a composite or a full-weight artifact: the class, its artifact
/// reference, its layout (carried: the chain's class row keeps only its digest) and its declarations.
pub fn improve_candidate_v1(
    line_id: Hash64,
    epoch: u64,
    class_id: Hash64,
    artifact: PalwTirArtifactRefV1,
    layout: kaspa_consensus_core::palw_tir_class_v1::PalwTirLayoutV1,
    declarations: PalwCandidateDeclarationsV1,
) -> PalwCandidateSubmissionV1 {
    PalwCandidateSubmissionV1 { line_id, epoch, class_id, artifact, layout, declarations }
}

/// **The unsigned objects a reveal is**: tag 74 (a setter set's prompts), tag 75 (its keys), tag 78 (a
/// teaching artifact).
pub fn improve_setter_prompts_object_v1(reveal: PalwSetterSetRevealV1) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::SetterSetRevealed { payload: Box::new(reveal) }
}

pub fn improve_setter_keys_object_v1(reveal: PalwSetterKeysRevealV1) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::SetterKeysRevealed { payload: Box::new(reveal) }
}

pub fn improve_artifact_reveal_object_v1(artifact: PalwTeachingArtifactV1) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::TeachingArtifactRevealed { payload: Box::new(artifact) }
}

// ---------------------------------------------------------------------------------------------
// The CLI's JSON specs
// ---------------------------------------------------------------------------------------------

/// **The JSON specs `palw-class improve` reads**, and the parsers of their fields. Ids are 128 lowercase hex
/// characters (an optional `0x` is tolerated), token ids arrays of integers.
pub mod spec {
    use super::*;
    use kaspa_consensus_core::palw_improve_state_v1::{
        PalwEvalSpecV1, PalwImprovementFeesV1, PalwJudgeSpecV1, PalwProvenancePolicyV1, PalwRollbackCauseV1, PalwScoringKindV1,
        PalwScoringParamsV1, PalwScoringStageV1, PalwTeacherClassV1, PalwTeachingArtifactKindV1, PalwUsageMeasureV1,
        PalwVerificationTypeV1,
    };
    use serde_json::Value;

    /// A 128-hex id.
    pub fn hash(s: &str) -> Result<Hash64, String> {
        let s = s.trim().trim_start_matches("0x");
        if s.len() != 128 {
            return Err(format!("{s:?} is not a 128-character hex id"));
        }
        s.parse::<Hash64>().map_err(|e| format!("{s:?}: {e:?}"))
    }

    fn field<'a>(v: &'a Value, k: &str) -> Result<&'a Value, String> {
        v.get(k).ok_or_else(|| format!("the spec has no `{k}`"))
    }

    fn hash_of(v: &Value, k: &str) -> Result<Hash64, String> {
        hash(field(v, k)?.as_str().ok_or_else(|| format!("`{k}` is not a string"))?)
    }

    fn u64_of(v: &Value, k: &str) -> Result<u64, String> {
        field(v, k)?.as_u64().ok_or_else(|| format!("`{k}` is not an unsigned integer"))
    }

    fn ids_of(v: &Value) -> Result<Vec<u32>, String> {
        v.as_array()
            .ok_or("not an array of token ids")?
            .iter()
            .map(|t| t.as_u64().and_then(|t| u32::try_from(t).ok()).ok_or_else(|| "a token id is not a u32".to_string()))
            .collect()
    }

    fn ids_at(v: &Value, k: &str) -> Result<Vec<u32>, String> {
        ids_of(field(v, k)?).map_err(|e| format!("`{k}`: {e}"))
    }

    /// The salt a spec names, or a fresh random one (returned so the caller can write it down).
    pub fn salt(v: &Value) -> Result<Hash64, String> {
        match v.get("salt") {
            Some(s) => hash(s.as_str().ok_or("`salt` is not a string")?),
            None => {
                let mut bytes = [0u8; 64];
                rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
                Ok(Hash64::from_bytes(bytes))
            }
        }
    }

    /// `txid:index` as a bond key.
    pub fn bond(s: &str) -> Result<PalwBondKeyV2, String> {
        let (txid, index) = s.split_once(':').ok_or_else(|| format!("--bond {s}: not <txid>:<index>"))?;
        Ok(PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
            txid.parse().map_err(|_| format!("--bond {s}: not a transaction id"))?,
            index.parse().map_err(|_| format!("--bond {s}: not an index"))?,
        )))
    }

    /// **A hard case's spec**: `{ line, domain, prompt: [ids], reference: null | {exact_key: [ids], salt?} |
    /// {continuation: [ids], salt?}, source: "setter" | {artifact: hex} | {usage_opt_in: hex}, head_evidence?: hex }`
    /// — the salt returned beside the case, since the key's reveal needs it.
    pub fn hard_case(v: &Value) -> Result<(PalwHardCaseV1, Option<(Vec<u32>, Hash64)>), String> {
        let line_id = hash_of(v, "line")?;
        let domain = v.get("domain").and_then(Value::as_u64).unwrap_or(0) as u16;
        let prompt = ids_at(v, "prompt")?;
        let (reference, opened) = match v.get("reference") {
            None | Some(Value::Null) => (PalwCaseReferenceV1::None, None),
            Some(r) => {
                let salt = salt(r)?;
                if let Some(key) = r.get("exact_key") {
                    let key = ids_of(key)?;
                    (improve_exact_key_reference_v1(&line_id, &key, &salt), Some((key, salt)))
                } else if let Some(c) = r.get("continuation") {
                    let ids = ids_of(c)?;
                    (improve_continuation_reference_v1(&line_id, &ids, &salt), Some((ids, salt)))
                } else {
                    return Err("`reference` is null, {exact_key} or {continuation}".into());
                }
            }
        };
        let source = match v.get("source") {
            None | Some(Value::Null) => PalwCaseSourceV1::Setter,
            Some(Value::String(s)) if s == "setter" => PalwCaseSourceV1::Setter,
            Some(o) if o.get("artifact").is_some() => PalwCaseSourceV1::Artifact { artifact_id: hash_of(o, "artifact")? },
            Some(o) if o.get("usage_opt_in").is_some() => PalwCaseSourceV1::UsageOptIn { job_pin: hash_of(o, "usage_opt_in")? },
            Some(_) => return Err("`source` is \"setter\", {artifact} or {usage_opt_in}".into()),
        };
        let head_evidence = match v.get("head_evidence") {
            None | Some(Value::Null) => None,
            Some(h) => Some(hash(h.as_str().ok_or("`head_evidence` is not a string")?)?),
        };
        Ok((improve_hard_case_v1(line_id, domain, prompt, reference, source, head_evidence), opened))
    }

    /// **A setter set's spec**: `{ line, epoch, prompts: [[ids]], keys: [[ids]], salt? }`.
    pub fn setter_set(v: &Value) -> Result<(PalwImproveSetterSetV1, Hash64), String> {
        let line_id = hash_of(v, "line")?;
        let epoch = u64_of(v, "epoch")?;
        let rows = |k: &str| -> Result<Vec<Vec<u32>>, String> {
            field(v, k)?.as_array().ok_or_else(|| format!("`{k}` is not an array"))?.iter().map(ids_of).collect()
        };
        let salt = salt(v)?;
        Ok((improve_setter_set_v1(line_id, epoch, rows("prompts")?, rows("keys")?, salt)?, salt))
    }

    /// **A dataset's spec**: `{ line, content_root, items, license_classes: [hex], teacher_classes: mask,
    /// provenance: hex }`.
    pub fn dataset(v: &Value) -> Result<PalwDatasetV1, String> {
        let classes: Vec<Hash64> = field(v, "license_classes")?
            .as_array()
            .ok_or("`license_classes` is not an array")?
            .iter()
            .map(|c| hash(c.as_str().ok_or("a licence class is not a string")?))
            .collect::<Result<_, _>>()?;
        Ok(improve_dataset_v1(
            hash_of(v, "line")?,
            hash_of(v, "content_root")?,
            u64_of(v, "items")?,
            classes,
            u64_of(v, "teacher_classes")? as u8,
            hash_of(v, "provenance")?,
        ))
    }

    /// **A teaching artifact's spec**: `{ line, kind: "answer"|"preference_pair"|"synthetic_problem"|
    /// "hard_case_variant"|"reward_signal"|"critique", task, teacher_type: "open_distill"|…, teacher_id,
    /// license_class, provenance, output_hash, verification: "exact"|"likelihood"|"judged"|"human",
    /// answer_span?: [ids], salt? }` — the artifact whole (its salt inside), the commitment and the reveal both made
    /// from it.
    pub fn teaching_artifact(v: &Value) -> Result<PalwTeachingArtifactV1, String> {
        let kind = match field(v, "kind")?.as_str().ok_or("`kind` is not a string")? {
            "answer" => PalwTeachingArtifactKindV1::Answer,
            "preference_pair" => PalwTeachingArtifactKindV1::PreferencePair,
            "synthetic_problem" => PalwTeachingArtifactKindV1::SyntheticProblem,
            "hard_case_variant" => PalwTeachingArtifactKindV1::HardCaseVariant,
            "reward_signal" => PalwTeachingArtifactKindV1::RewardSignal,
            "critique" => PalwTeachingArtifactKindV1::Critique,
            other => return Err(format!("unknown artifact kind {other:?}")),
        };
        let teacher_type = match field(v, "teacher_type")?.as_str().ok_or("`teacher_type` is not a string")? {
            "open_distill" => PalwTeacherClassV1::OpenDistill,
            "licensed_distill" => PalwTeacherClassV1::LicensedDistill,
            "self_play" => PalwTeacherClassV1::SelfPlay,
            "human" => PalwTeacherClassV1::Human,
            "tool_verified" => PalwTeacherClassV1::ToolVerified,
            "public_data" => PalwTeacherClassV1::PublicData,
            other => return Err(format!("unknown teacher type {other:?}")),
        };
        let verification_type = match field(v, "verification")?.as_str().ok_or("`verification` is not a string")? {
            "exact" => PalwVerificationTypeV1::Exact,
            "likelihood" => PalwVerificationTypeV1::Likelihood,
            "judged" => PalwVerificationTypeV1::Judged,
            "human" => PalwVerificationTypeV1::Human,
            other => return Err(format!("unknown verification type {other:?}")),
        };
        Ok(PalwTeachingArtifactV1 {
            line_id: hash_of(v, "line")?,
            kind,
            task_id: hash_of(v, "task")?,
            teacher_type,
            teacher_id: hash_of(v, "teacher_id")?,
            license_class: hash_of(v, "license_class")?,
            provenance_commitment: hash_of(v, "provenance")?,
            output_hash: hash_of(v, "output_hash")?,
            verification_type,
            answer_span: match v.get("answer_span") {
                Some(a) => ids_of(a)?,
                None => Vec::new(),
            },
            salt: salt(v)?,
        })
    }

    /// **A candidate's declarations**: `{ datasets: [[hex, weight_permille]], licences: [hex], teacher_classes: mask }`
    /// (each optional).
    pub fn declarations(v: &Value) -> Result<PalwCandidateDeclarationsV1, String> {
        let datasets = match v.get("datasets") {
            None | Some(Value::Null) => Vec::new(),
            Some(d) => d
                .as_array()
                .ok_or("`datasets` is not an array")?
                .iter()
                .map(|row| {
                    let a = row.as_array().filter(|a| a.len() == 2).ok_or("a dataset row is [id, weight_permille]")?;
                    Ok((
                        hash(a[0].as_str().ok_or("a dataset id is not a string")?)?,
                        a[1].as_u64().and_then(|w| u16::try_from(w).ok()).ok_or("a dataset weight is not a u16")?,
                    ))
                })
                .collect::<Result<_, String>>()?,
        };
        let licences = match v.get("licences") {
            None | Some(Value::Null) => Vec::new(),
            Some(l) => l
                .as_array()
                .ok_or("`licences` is not an array")?
                .iter()
                .map(|h| hash(h.as_str().ok_or("a licence id is not a string")?))
                .collect::<Result<_, _>>()?,
        };
        Ok(PalwCandidateDeclarationsV1 {
            datasets,
            licences,
            teacher_classes: v.get("teacher_classes").and_then(Value::as_u64).unwrap_or(0) as u8,
        })
    }

    /// **A rollback's cause**: `"owner"`, `{later_regression: epoch}`, `{canary_failed: {claim, item}}`,
    /// `{licence_violation: hex}`.
    pub fn rollback_cause(v: &Value) -> Result<PalwRollbackCauseV1, String> {
        match v {
            Value::String(s) if s == "owner" => Ok(PalwRollbackCauseV1::Owner),
            o if o.get("later_regression").is_some() => {
                Ok(PalwRollbackCauseV1::LaterRegression { epoch: u64_of(o, "later_regression")? })
            }
            o if o.get("canary_failed").is_some() => {
                let c = field(o, "canary_failed")?;
                Ok(PalwRollbackCauseV1::CanaryFailed { claim: hash_of(c, "claim")?, item: hash_of(c, "item")? })
            }
            o if o.get("licence_violation").is_some() => Ok(PalwRollbackCauseV1::LicenceViolation { challenge: hash_of(o, "licence_violation")? }),
            _ => Err("a cause is \"owner\", {later_regression}, {canary_failed} or {licence_violation}".into()),
        }
    }

    // ---- the policy: the drill example with the spec's overrides ----

    fn set_u64(slot: &mut u64, v: &Value, k: &str) -> Result<(), String> {
        if let Some(x) = v.get(k) {
            *slot = x.as_u64().ok_or_else(|| format!("`{k}` is not an unsigned integer"))?;
        }
        Ok(())
    }

    fn set_u32(slot: &mut u32, v: &Value, k: &str) -> Result<(), String> {
        let mut wide = u64::from(*slot);
        set_u64(&mut wide, v, k)?;
        *slot = u32::try_from(wide).map_err(|_| format!("`{k}` does not fit u32"))?;
        Ok(())
    }

    fn set_u16(slot: &mut u16, v: &Value, k: &str) -> Result<(), String> {
        let mut wide = u64::from(*slot);
        set_u64(&mut wide, v, k)?;
        *slot = u16::try_from(wide).map_err(|_| format!("`{k}` does not fit u16"))?;
        Ok(())
    }

    /// **A policy**: the network's example (`palw_improvement_policy_example_v1`) with the spec's values over
    /// it. Sections: `usage {measure: "claims"|"work_leaves", value}`, `windows {grid, w_collect, w_submit,
    /// w_holdout, w_eval, beacon_delay, court_margin}`, `eval {stages: [{kind: "exact_match", open, close,
    /// key_cap} | {kind: "ref_loglik", logit_scale_q24} | {kind: "judge", lo, hi} | {kind: "pairwise", margin}],
    /// n, n_min, delta_permille, epsilon_permille, epsilon_safety_permille, alpha_permille, max_new_tokens,
    /// stop_ids, setter_cap_permille, max_eval_positions, regression_items, regression_dataset, safety_items,
    /// safety_dataset, judge_set, judge {template_dataset, verdict_a, verdict_b, logit_scale_q24} (null: none),
    /// pairwise {…} (likewise), seat_pool_permille, anchor_floor_permille}`, `k_max`, `fees {…}`, the permille shares,
    /// `provenance {teacher_classes, licence_classes, full_weight_candidates, base_licence_class}`,
    /// `rollback_epochs`, `vest_epochs`, `ban_epochs`. The chain's own check
    /// (`palw_improvement_policy_check_v1`) says whether the result is admissible.
    pub fn policy(v: &Value) -> Result<PalwImprovementPolicyV1, String> {
        let mut p = kaspa_consensus_core::palw_improve_policy_v1::palw_improvement_policy_example_v1();
        if let Some(u) = v.get("usage") {
            if let Some(m) = u.get("measure") {
                p.usage.measure = match m.as_str() {
                    Some("claims") => PalwUsageMeasureV1::Claims,
                    Some("work_leaves") => PalwUsageMeasureV1::WorkLeaves,
                    _ => return Err("`usage.measure` is \"claims\" or \"work_leaves\"".into()),
                };
            }
            if let Some(x) = u.get("value") {
                p.usage.value = u128::from(x.as_u64().ok_or("`usage.value` is not an unsigned integer")?);
            }
        }
        if let Some(w) = v.get("windows") {
            let c = &mut p.windows;
            for (slot, k) in [
                (&mut c.grid, "grid"),
                (&mut c.w_collect, "w_collect"),
                (&mut c.w_submit, "w_submit"),
                (&mut c.w_holdout, "w_holdout"),
                (&mut c.w_eval, "w_eval"),
                (&mut c.beacon_delay, "beacon_delay"),
                (&mut c.court_margin, "court_margin"),
            ] {
                set_u64(slot, w, k)?;
            }
        }
        if let Some(e) = v.get("eval") {
            let s: &mut PalwEvalSpecV1 = &mut p.eval;
            if let Some(stages) = e.get("stages") {
                s.stages = stages
                    .as_array()
                    .ok_or("`eval.stages` is not an array")?
                    .iter()
                    .map(|st| {
                        let i32_of = |k: &str| -> Result<i32, String> {
                            field(st, k)?.as_i64().and_then(|x| i32::try_from(x).ok()).ok_or_else(|| format!("`{k}` is not an i32"))
                        };
                        Ok(match field(st, "kind")?.as_str().ok_or("a stage's `kind` is not a string")? {
                            "exact_match" => PalwScoringStageV1 {
                                kind: PalwScoringKindV1::ExactMatch,
                                params: PalwScoringParamsV1::ExactMatch {
                                    open: i32_of("open")?,
                                    close: i32_of("close")?,
                                    key_cap: u32::try_from(u64_of(st, "key_cap")?).map_err(|_| "`key_cap` does not fit u32")?,
                                },
                            },
                            "ref_loglik" => PalwScoringStageV1 {
                                kind: PalwScoringKindV1::RefLogLik,
                                params: PalwScoringParamsV1::RefLogLik { logit_scale_q24: i32_of("logit_scale_q24")? },
                            },
                            "judge" => PalwScoringStageV1 {
                                kind: PalwScoringKindV1::Judge,
                                params: PalwScoringParamsV1::Judge { lo: i32_of("lo")?, hi: i32_of("hi")? },
                            },
                            "pairwise" => PalwScoringStageV1 {
                                kind: PalwScoringKindV1::Pairwise,
                                params: PalwScoringParamsV1::Pairwise { margin: i32_of("margin")? },
                            },
                            other => return Err(format!("unknown stage kind {other:?}")),
                        })
                    })
                    .collect::<Result<_, String>>()?;
            }
            for (slot, k) in [
                (&mut s.regression_items, "regression_items"),
                (&mut s.safety_items, "safety_items"),
                (&mut s.n, "n"),
                (&mut s.n_min, "n_min"),
                (&mut s.max_new_tokens, "max_new_tokens"),
            ] {
                set_u32(slot, e, k)?;
            }
            for (slot, k) in [
                (&mut s.anchor_floor_permille, "anchor_floor_permille"),
                (&mut s.delta_permille, "delta_permille"),
                (&mut s.epsilon_permille, "epsilon_permille"),
                (&mut s.epsilon_safety_permille, "epsilon_safety_permille"),
                (&mut s.alpha_permille, "alpha_permille"),
                (&mut s.setter_cap_permille, "setter_cap_permille"),
            ] {
                set_u16(slot, e, k)?;
            }
            set_u64(&mut s.max_eval_positions, e, "max_eval_positions")?;
            if let Some(ids) = e.get("stop_ids") {
                s.stop_ids = ids_of(ids)?;
            }
            // A suite with no items names no dataset (the chain's check): naming zero items clears the example's dataset.
            if e.get("regression_dataset").is_some() {
                s.regression_dataset = hash_of(e, "regression_dataset")?;
            } else if s.regression_items == 0 {
                s.regression_dataset = Hash64::default();
            }
            if e.get("safety_dataset").is_some() {
                s.safety_dataset = hash_of(e, "safety_dataset")?;
            } else if s.safety_items == 0 {
                s.safety_dataset = Hash64::default();
            }
            if let Some(j) = e.get("judge_set") {
                s.judge_set =
                    j.as_array().ok_or("`eval.judge_set` is not an array")?.iter().map(|h| hash(h.as_str().ok_or("a judge is not a string")?)).collect::<Result<_, _>>()?;
            }
            // A judged stage's specification (spec 17 §17.8.5): `null` (or no key) leaves the example's, which has none.
            for (key, slot) in [("judge", &mut s.judge), ("pairwise", &mut s.pairwise)] {
                match e.get(key) {
                    None => {}
                    Some(Value::Null) => *slot = None,
                    Some(j) => {
                        *slot = Some(PalwJudgeSpecV1 {
                            template_dataset: hash_of(j, "template_dataset").map_err(|e| format!("`eval.{key}`: {e}"))?,
                            verdict_a: ids_at(j, "verdict_a").map_err(|e| format!("`eval.{key}`: {e}"))?,
                            verdict_b: ids_at(j, "verdict_b").map_err(|e| format!("`eval.{key}`: {e}"))?,
                            logit_scale_q24: field(j, "logit_scale_q24")
                                .map_err(|e| format!("`eval.{key}`: {e}"))?
                                .as_i64()
                                .and_then(|x| i32::try_from(x).ok())
                                .ok_or_else(|| format!("`eval.{key}.logit_scale_q24` is not an i32"))?,
                        });
                    }
                }
            }
            set_u16(&mut s.seat_pool_permille, e, "seat_pool_permille")?;
        }
        if let Some(k) = v.get("k_max") {
            p.k_max = k.as_u64().and_then(|k| u8::try_from(k).ok()).ok_or("`k_max` is not a u8")?;
        }
        if let Some(f) = v.get("fees") {
            let c: &mut PalwImprovementFeesV1 = &mut p.fees;
            for (slot, k) in [
                (&mut c.registration_fee, "registration_fee"),
                (&mut c.candidate_bond, "candidate_bond"),
                (&mut c.eval_fee_per_job, "eval_fee_per_job"),
                (&mut c.hard_case_fee, "hard_case_fee"),
                (&mut c.artifact_bond, "artifact_bond"),
                (&mut c.setter_bond, "setter_bond"),
                (&mut c.dataset_bond, "dataset_bond"),
                (&mut c.s1_bounty, "s1_bounty"),
                (&mut c.s1_setter_reward, "s1_setter_reward"),
            ] {
                set_u64(slot, f, k)?;
            }
        }
        for (slot, k) in [
            (&mut p.phi_permille, "phi_permille"),
            (&mut p.bounty_share_permille, "bounty_share_permille"),
            (&mut p.promotion_share_permille, "promotion_share_permille"),
            (&mut p.s2_trainer_permille, "s2_trainer_permille"),
            (&mut p.s2_dataset_cap_permille, "s2_dataset_cap_permille"),
            (&mut p.s2_contributor_cap_permille, "s2_contributor_cap_permille"),
        ] {
            set_u16(slot, v, k)?;
        }
        if let Some(pr) = v.get("provenance") {
            let c: &mut PalwProvenancePolicyV1 = &mut p.provenance;
            if let Some(m) = pr.get("teacher_classes") {
                c.teacher_classes = m.as_u64().and_then(|m| u8::try_from(m).ok()).ok_or("`provenance.teacher_classes` is not a u8")?;
            }
            if let Some(l) = pr.get("licence_classes") {
                c.licence_classes = l
                    .as_array()
                    .ok_or("`provenance.licence_classes` is not an array")?
                    .iter()
                    .map(|h| hash(h.as_str().ok_or("a licence class is not a string")?))
                    .collect::<Result<_, _>>()?;
            }
            if let Some(f) = pr.get("full_weight_candidates") {
                c.full_weight_candidates = f.as_bool().ok_or("`provenance.full_weight_candidates` is not a boolean")?;
            }
            if pr.get("base_licence_class").is_some() {
                c.base_licence_class = hash_of(pr, "base_licence_class")?;
            }
        }
        for (slot, k) in [(&mut p.rollback_epochs, "rollback_epochs"), (&mut p.vest_epochs, "vest_epochs"), (&mut p.ban_epochs, "ban_epochs")] {
            set_u32(slot, v, k)?;
        }
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_improve_material_v1::{
        palw_dataset_form_v1, palw_hard_case_form_v1, palw_setter_keys_open_v1, palw_setter_prompts_open_v1, palw_setter_set_form_v1,
        palw_teacher_licence_form_v1, palw_teaching_artifact_form_v1,
    };
    use kaspa_consensus_core::palw_improve_policy_v1::palw_improvement_policy_check_v1;
    use kaspa_consensus_core::palw_improve_v1::PALW_DRILL_IMPROVE_CEILINGS_V1;

    fn key() -> ValidatorKey {
        ValidatorKey::from_seed([0x42; 32])
    }

    fn signer(key: &ValidatorKey) -> PalwImproveSignerV1<'_> {
        PalwImproveSignerV1 { key, network_domain: Hash64::from_bytes([0x10; 64]) }
    }

    fn bond(n: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint {
            transaction_id: kaspa_consensus_core::tx::TransactionId::from_bytes([n; 64]),
            index: n as u32,
        })
    }

    fn verifies(public: &[u8], message: Hash64, signature: &[u8], context: &[u8]) -> bool {
        matches!(kaspa_txscript::verify_mldsa87_with_context(public, message.as_byte_slice(), signature, context), Ok(true))
    }

    /// **Every signed object verifies under the chain's own message and context**, and not under another's:
    /// the policy under the owner's key and the policy context, the material objects under the material
    /// context over their tag and bond, the candidate under its own message, the rollback under the filer's
    /// — and the derived ids are the ones the chain's form checks derive.
    #[test]
    fn each_object_is_signed_as_the_chain_verifies_it_and_its_ids_are_derived() {
        let (k, line) = (key(), Hash64::from_bytes([0x11; 64]));
        let s = signer(&k);
        let domain = s.network_domain;
        let public = k.public_key().to_vec();

        // Tag 70.
        let policy = kaspa_consensus_core::palw_improve_policy_v1::palw_improvement_policy_example_v1();
        let PalwConsensusObjectV2::ModelLineImprovementPolicySet { payload, signature } = s.policy(line, 1, Some(policy.clone())) else {
            panic!("tag 70")
        };
        let message = palw_improvement_policy_message_v1(domain, &line, 1, payload.policy.as_ref());
        assert!(verifies(&public, message, &signature, PALW_IMPROVE_POLICY_MLDSA87_CONTEXT));
        assert!(!verifies(&public, message, &signature, PALW_IMPROVE_ROLLBACK_MLDSA87_CONTEXT), "not under another context");
        assert_eq!(palw_improvement_policy_check_v1(payload.policy.as_ref().unwrap(), &PALW_DRILL_IMPROVE_CEILINGS_V1, None), Ok(()));

        // Tag 71: the case id is derived and the form checks.
        let salt = Hash64::from_bytes([0x77; 64]);
        let reference = improve_exact_key_reference_v1(&line, &[7, 8], &salt);
        let case = improve_hard_case_v1(line, 0, vec![1, 2, 3], reference, PalwCaseSourceV1::Setter, None);
        assert_eq!(palw_hard_case_form_v1(&case), Ok(()));
        let PalwConsensusObjectV2::HardCaseSubmitted { payload, submitter, signature } = s.hard_case(bond(3), case) else { panic!("tag 71") };
        let message = palw_improve_material_message_v1(71, &domain, payload.as_ref(), Some(&submitter));
        assert!(verifies(&public, message, &signature, PALW_IMPROVE_MATERIAL_MLDSA87_CONTEXT_V1));
        assert!(!verifies(&public, palw_improve_material_message_v1(72, &domain, payload.as_ref(), Some(&submitter)), &signature, PALW_IMPROVE_MATERIAL_MLDSA87_CONTEXT_V1));

        // Tags 73, 74, 75: one set, two reveals that open its commitments.
        let set = improve_setter_set_v1(line, 2, vec![vec![1, 2], vec![3]], vec![vec![9], vec![]], salt).unwrap();
        assert_eq!(palw_setter_set_form_v1(&set.commitment), Ok(()));
        assert_eq!(palw_setter_prompts_open_v1(&set.commitment, &set.prompts), Ok(()));
        assert_eq!(palw_setter_keys_open_v1(&set.commitment, &set.keys), Ok(()));
        assert!(improve_setter_set_v1(line, 2, vec![vec![1]], vec![], salt).is_err(), "one key per prompt");
        let PalwConsensusObjectV2::SetterSetCommitted { payload, setter, signature } = s.setter_set(bond(4), set.commitment) else {
            panic!("tag 73")
        };
        assert!(verifies(&public, palw_improve_material_message_v1(73, &domain, payload.as_ref(), Some(&setter)), &signature, PALW_IMPROVE_MATERIAL_MLDSA87_CONTEXT_V1));
        assert!(matches!(improve_setter_prompts_object_v1(set.prompts), PalwConsensusObjectV2::SetterSetRevealed { .. }));
        assert!(matches!(improve_setter_keys_object_v1(set.keys), PalwConsensusObjectV2::SetterKeysRevealed { .. }));

        // Tag 76.
        let dataset = improve_dataset_v1(line, Hash64::from_bytes([5; 64]), 10, vec![Hash64::from_bytes([9; 64]), Hash64::from_bytes([8; 64])], 0b101, Hash64::from_bytes([6; 64]));
        assert_eq!(palw_dataset_form_v1(&dataset), Ok(()), "sorted licence classes, the id derived");
        let PalwConsensusObjectV2::DatasetRegistered { payload, registrant, signature } = s.dataset(bond(5), dataset) else { panic!("tag 76") };
        assert!(verifies(&public, palw_improve_material_message_v1(76, &domain, payload.as_ref(), Some(&registrant)), &signature, PALW_IMPROVE_MATERIAL_MLDSA87_CONTEXT_V1));

        // Tags 77, 78.
        let artifact = PalwTeachingArtifactV1 {
            line_id: line,
            kind: kaspa_consensus_core::palw_improve_state_v1::PalwTeachingArtifactKindV1::Answer,
            task_id: Hash64::from_bytes([1; 64]),
            teacher_type: kaspa_consensus_core::palw_improve_state_v1::PalwTeacherClassV1::OpenDistill,
            teacher_id: Hash64::from_bytes([2; 64]),
            license_class: Hash64::from_bytes([3; 64]),
            provenance_commitment: Hash64::from_bytes([4; 64]),
            output_hash: Hash64::from_bytes([5; 64]),
            verification_type: kaspa_consensus_core::palw_improve_state_v1::PalwVerificationTypeV1::Exact,
            answer_span: vec![7, 8],
            salt,
        };
        assert_eq!(palw_teaching_artifact_form_v1(&artifact), Ok(()));
        let commit = improve_teaching_commit_v1(&artifact);
        assert_eq!(commit.commit, palw_teaching_artifact_commit_v1(&artifact), "the reveal opens the commitment");
        assert!(matches!(s.teaching_artifact(bond(6), commit), PalwConsensusObjectV2::TeachingArtifactCommitted { .. }));
        assert!(matches!(improve_artifact_reveal_object_v1(artifact), PalwConsensusObjectV2::TeachingArtifactRevealed { .. }));

        // Tag 79: signed by the rights holder's own key, which the licence names.
        let licence = improve_licence_v1(public.clone(), Hash64::from_bytes([7; 64]), vec![3, 1, 3], 1, 5, 10_000);
        assert_eq!(palw_teacher_licence_form_v1(&licence), Ok(()));
        let PalwConsensusObjectV2::TeacherLicenceRegistered { payload, signature } = s.licence(licence).unwrap() else { panic!("tag 79") };
        assert!(verifies(&public, palw_improve_material_message_v1(79, &domain, payload.as_ref(), None), &signature, PALW_IMPROVE_MATERIAL_MLDSA87_CONTEXT_V1));
        let stranger = improve_licence_v1(vec![0; 2592], Hash64::from_bytes([7; 64]), vec![], 1, 5, 10_000);
        assert!(s.licence(stranger).is_err(), "a licence signed by another key than the one it names");

        // Tag 80.
        let layout = kaspa_consensus_core::palw_tir_class_v1::PalwTirLayoutV1 {
            version: kaspa_consensus_core::palw_tir_class_v1::PALW_TIR_LAYOUT_VERSION_V1,
            max_context: 16,
            checkpoint_interval: 1,
            h_tile: 4,
            commit_tiles: vec![],
            state_tiles: vec![],
        };
        let candidate = improve_candidate_v1(
            line,
            2,
            Hash64::from_bytes([0xC1; 64]),
            PalwTirArtifactRefV1::Composite { parent_class: Hash64::from_bytes([1; 64]), parent_root: Hash64::from_bytes([2; 64]), adapter_root: Hash64::from_bytes([3; 64]), p: 40 },
            layout,
            PalwCandidateDeclarationsV1 { datasets: vec![], licences: vec![], teacher_classes: 0 },
        );
        let PalwConsensusObjectV2::CandidateSubmitted { payload, submitter, signature } = s.candidate(bond(7), candidate) else { panic!("tag 80") };
        assert!(verifies(&public, palw_candidate_submission_message_v1(&domain, payload.as_ref(), &submitter), &signature, PALW_IMPROVE_CANDIDATE_MLDSA87_CONTEXT_V1));

        // Tag 81.
        let rollback = PalwLineageRollbackV1 {
            line_id: line,
            epoch: 3,
            to_class: Hash64::from_bytes([9; 64]),
            cause: kaspa_consensus_core::palw_improve_state_v1::PalwRollbackCauseV1::Owner,
        };
        let PalwConsensusObjectV2::LineageHeadRolledBack { payload, signature, .. } = s.rollback(bond(8), rollback) else { panic!("tag 81") };
        assert!(verifies(&public, palw_improvement_rollback_message_v1(domain, payload.as_ref()), &signature, PALW_IMPROVE_ROLLBACK_MLDSA87_CONTEXT));
    }

    /// **The JSON specs**: a policy is the example with the spec's values over it and passes the chain's
    /// own check; a hard case's reference is committed with its salt and the key returned for the reveal; a
    /// setter set, a dataset, an artifact and a cause parse; a malformed spec is refused by name.
    #[test]
    fn the_specs_parse_into_the_chains_payloads_and_a_bad_one_is_refused() {
        let hex = |b: u8| Hash64::from_bytes([b; 64]).to_string();
        let policy = spec::policy(&serde_json::json!({
            "usage": { "measure": "claims", "value": 3 },
            "windows": { "grid": 400, "w_collect": 60, "w_submit": 60, "w_holdout": 20, "w_eval": 120, "beacon_delay": 4, "court_margin": 60 },
            "eval": {
                "stages": [{ "kind": "exact_match", "open": -1, "close": -1, "key_cap": 4 }],
                "n": 8, "n_min": 4, "delta_permille": 100, "alpha_permille": 50, "max_new_tokens": 4, "stop_ids": [],
                "regression_items": 0, "safety_items": 0, "setter_cap_permille": 1000, "max_eval_positions": 100000
            },
            "k_max": 2,
            "fees": { "eval_fee_per_job": 10 },
            "provenance": { "full_weight_candidates": false }
        }))
        .expect("a policy");
        assert_eq!(policy.windows.grid, 400);
        assert_eq!(policy.eval.n, 8);
        assert_eq!(policy.fees.eval_fee_per_job, 10);
        assert_eq!(policy.fees.registration_fee, 100_000_000, "the example's value stands where the spec says nothing");
        assert_eq!(palw_improvement_policy_check_v1(&policy, &PALW_DRILL_IMPROVE_CEILINGS_V1, None), Ok(()), "the drill's ceilings admit it");
        assert!(spec::policy(&serde_json::json!({ "windows": { "grid": "wide" } })).is_err());
        assert!(spec::policy(&serde_json::json!({ "eval": { "stages": [{ "kind": "nonsense" }] } })).is_err());

        let (case, opened) = spec::hard_case(&serde_json::json!({
            "line": hex(1), "prompt": [1, 2, 3], "reference": { "exact_key": [5, 6], "salt": hex(9) }
        }))
        .expect("a case");
        let (key, salt) = opened.expect("the key and its salt come back");
        assert_eq!((key, salt), (vec![5, 6], Hash64::from_bytes([9; 64])));
        assert_eq!(case.reference, improve_exact_key_reference_v1(&Hash64::from_bytes([1; 64]), &[5, 6], &Hash64::from_bytes([9; 64])));
        assert_eq!(palw_hard_case_form_v1(&case), Ok(()));
        assert!(spec::hard_case(&serde_json::json!({ "line": hex(1), "prompt": [1], "reference": { "wat": 1 } })).is_err());

        let (set, salt) = spec::setter_set(&serde_json::json!({ "line": hex(1), "epoch": 1, "prompts": [[1, 2], [3]], "keys": [[7], [8]] })).expect("a set");
        assert_eq!(palw_setter_prompts_open_v1(&set.commitment, &set.prompts), Ok(()));
        assert_eq!(set.prompts.salt, salt, "a drawn salt is the reveal's");

        let dataset = spec::dataset(&serde_json::json!({
            "line": hex(1), "content_root": hex(2), "items": 4, "license_classes": [hex(3)], "teacher_classes": 1, "provenance": hex(4)
        }))
        .expect("a dataset");
        assert_eq!(palw_dataset_form_v1(&dataset), Ok(()));
        let artifact = spec::teaching_artifact(&serde_json::json!({
            "line": hex(1), "kind": "answer", "task": hex(2), "teacher_type": "open_distill", "teacher_id": hex(3),
            "license_class": hex(4), "provenance": hex(5), "output_hash": hex(6), "verification": "exact", "answer_span": [1]
        }))
        .expect("an artifact");
        assert_eq!(palw_teaching_artifact_form_v1(&artifact), Ok(()));
        let d = spec::declarations(&serde_json::json!({ "datasets": [[hex(1), 600]], "teacher_classes": 4 })).expect("declarations");
        assert_eq!(d.datasets, vec![(Hash64::from_bytes([1; 64]), 600)]);
        assert_eq!(spec::rollback_cause(&serde_json::json!("owner")).unwrap(), kaspa_consensus_core::palw_improve_state_v1::PalwRollbackCauseV1::Owner);
        assert!(spec::rollback_cause(&serde_json::json!("whim")).is_err());
        assert!(spec::hash("abc").is_err());
        assert_eq!(spec::bond(&format!("{}:3", hex(2))).unwrap().0.index, 3);
        assert!(spec::bond("nonsense").is_err());
    }
}
