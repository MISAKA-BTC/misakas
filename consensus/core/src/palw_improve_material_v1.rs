//! **RFC-0004 §5, §9, §10: the training and evaluation material's wire payloads** — hard cases, the
//! data-use opt-in, setter sets, registered datasets, teaching artifacts and teacher licences (tags
//! 71–79).
//!
//! Skeleton (step 0) of the candidates lane's module (`rfc4/cand` owns it from the skeleton's sha on):
//! the payloads' shapes. Their validation and admission are the lane's (A4).

use crate::Hash64;
use crate::palw_improve_state_v1::{PalwTeacherClassV1, PalwTeachingArtifactKindV1, PalwVerificationTypeV1};
use borsh::{BorshDeserialize, BorshSerialize};

/// What a hard case is scored against (RFC-0004 §5.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwCaseReferenceV1 {
    /// An exact-match task: the committed answer span.
    ExactKey { commitment: Hash64 },
    /// A likelihood task: the committed reference continuation.
    Continuation { commitment: Hash64 },
    /// Judged only.
    None,
}

/// Where a hard case came from (RFC-0004 §5.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwCaseSourceV1 {
    /// Real usage, under the job's `DataUseOptIn`.
    UsageOptIn { job_pin: Hash64 },
    /// A bonded problem setter.
    Setter,
    /// A `SyntheticProblem` or `HardCaseVariant` artifact.
    Artifact { artifact_id: Hash64 },
}

/// **`HardCaseSubmitted` (tag 71).**
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwHardCaseV1 {
    pub line_id: Hash64,
    pub case_id: Hash64,
    pub domain: u16,
    /// Token ids under the line's tokenizer.
    pub prompt_ids: Vec<u32>,
    pub reference: PalwCaseReferenceV1,
    pub source: PalwCaseSourceV1,
    /// A final evaluation-kind claim of the head that fails the case's reference, when the case claims hardness.
    pub head_evidence: Option<Hash64>,
}

/// **What a free-prompt job's pin is over** (ADR-0152 v3.1 J-1, `palw_fp_job_pin_v1`): the seven
/// identity facts the claim's `job_identity` hashes — carried by an opt-in so the chain can recompute
/// the pin and read the job's committed prompt hash from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwFpJobFactsV1 {
    pub job_id: Hash64,
    pub execution_seed: [u8; 32],
    pub tokenizer_id: Hash64,
    pub prompt_token_ids_hash: Hash64,
    pub prompt_tokens: u32,
    pub decode_tokens_executed: u32,
    pub max_context_tokens: u32,
}

impl PalwFpJobFactsV1 {
    /// **The facts of a free-prompt commitment** — what a committer's opt-in carries: the same seven
    /// values `palw_fp_job_pin_v1` hashes, so `of_commitment(c).pin() == palw_fp_job_pin_v1(c)`.
    pub fn of_commitment(c: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3) -> Self {
        Self {
            job_id: crate::palw_freeprompt_v3::fp_job_id_v3(&c.job),
            execution_seed: crate::palw_fp_execution_v3::palw_fp_execution_seed_v3(&c.job),
            tokenizer_id: c.job.tokenizer_id,
            prompt_token_ids_hash: c.job.prompt_token_ids_hash,
            prompt_tokens: c.job.prompt_tokens,
            decode_tokens_executed: c.decode_tokens_executed,
            max_context_tokens: c.job.max_context_tokens,
        }
    }

    /// **The job pin these facts hash to** — `palw_fp_job_pin_of_context_v1`, the one spelling the
    /// claim path records (it reads these seven fields of a context and nothing else).
    pub fn pin(&self) -> Hash64 {
        let zero = Hash64::from_bytes([0; 64]);
        let ctx = crate::palw_v2::PalwJobContextV2 {
            version: 2,
            network_id: Vec::new(),
            job_id: self.job_id,
            job_nullifier: zero,
            assignment_id: zero,
            execution_seed: self.execution_seed,
            model_profile_id: zero,
            runtime_manifest_hash: zero,
            runtime_class_id: zero,
            shape_profile_id: zero,
            trace_scheme_id: zero,
            cu_ruleset_id: zero,
            tokenizer_id: self.tokenizer_id,
            prompt_token_ids_hash: self.prompt_token_ids_hash,
            declared_prefill_tokens: self.prompt_tokens,
            exact_decode_tokens: self.decode_tokens_executed,
            max_context_tokens: self.max_context_tokens,
        };
        crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&ctx)
    }
}

/// **`DataUseOptIn` (tag 72)** (RFC-0004 §10; PALW-MIP-19): a job's prompt made usable as a hard case.
/// Signed by the job's committer — the bond of `claim`, the free-prompt claim whose `job_identity` is
/// `job_pin`; `job` is the pin's preimage, from which the chain reads the prompt's commitment.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwDataUseOptInV1 {
    pub job_pin: Hash64,
    pub claim: Hash64,
    pub job: PalwFpJobFactsV1,
}

/// **`SetterSetCommitted` (tag 73)** (RFC-0004 §7.1): before `t_close`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSetterSetCommitmentV1 {
    pub line_id: Hash64,
    pub epoch: u64,
    pub set_id: Hash64,
    pub items: u32,
    pub prompts_commitment: Hash64,
    pub keys_commitment: Hash64,
}

/// **`SetterSetRevealed` (tag 74)**: the prompts, at `Drawn`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSetterSetRevealV1 {
    pub line_id: Hash64,
    pub epoch: u64,
    pub set_id: Hash64,
    pub prompts: Vec<Vec<u32>>,
    pub salt: Hash64,
}

/// **`SetterKeysRevealed` (tag 75)**: the keys and references, after every subject's outputs are final.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSetterKeysRevealV1 {
    pub line_id: Hash64,
    pub epoch: u64,
    pub set_id: Hash64,
    pub keys: Vec<Vec<u32>>,
    pub salt: Hash64,
}

/// **`DatasetRegistered` (tag 76)** (RFC-0004 §5.3).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwDatasetV1 {
    pub line_id: Hash64,
    pub dataset_id: Hash64,
    pub content_root: Hash64,
    pub items: u64,
    pub license_classes: Vec<Hash64>,
    /// A mask of [`PalwTeacherClassV1::bit`].
    pub teacher_classes: u8,
    pub provenance_commitment: Hash64,
}

/// **`TeachingArtifactCommitted` (tag 77)**: `commit = H(artifact ‖ salt)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTeachingArtifactCommitV1 {
    pub line_id: Hash64,
    pub commit: Hash64,
}

/// **`TeachingArtifactRevealed` (tag 78)** (RFC-0004 §5.3).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTeachingArtifactV1 {
    pub line_id: Hash64,
    pub kind: PalwTeachingArtifactKindV1,
    pub task_id: Hash64,
    pub teacher_type: PalwTeacherClassV1,
    pub teacher_id: Hash64,
    pub license_class: Hash64,
    pub provenance_commitment: Hash64,
    /// The content's hash; the content lives off chain, content-addressed.
    pub output_hash: Hash64,
    pub verification_type: PalwVerificationTypeV1,
    /// For an EXACT-verified `Answer`: the answer span the fold compares with the key.
    pub answer_span: Vec<u32>,
    pub salt: Hash64,
}

/// **`TeacherLicenceRegistered` (tag 79)** (RFC-0004 §9): a rights holder's licence for `LICENSED_DISTILL`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTeacherLicenceV1 {
    pub licence_id: Hash64,
    /// The rights holder's ML-DSA-87 public key.
    pub rights_holder_key: Vec<u8>,
    pub model_family: Hash64,
    pub domains: Vec<u16>,
    /// A mask of permitted uses (bit 0: training data).
    pub uses: u8,
    pub per_use_fee: u64,
    pub expiry_daa: u64,
}

/// **`HardCaseKeyRevealed` (tag 86)** (spec 17 §17.0, §17.8.2): a case's committed key (`ExactKey`) or
/// reference continuation (`Continuation`), opened. Unsigned: only the salt opens the commitment.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwCaseKeyRevealV1 {
    pub line_id: Hash64,
    pub case_id: Hash64,
    pub key: Vec<u32>,
    pub salt: Hash64,
}

// =================================================================================================
// A4's pure rules: ids, commitments, signed messages, forms, records (RFC-0004 §5, §7.1, §9, §10)
// =================================================================================================

/// Key of a hard case's id.
pub const PALW_IMPROVE_CASE_DOMAIN_V1: &[u8] = b"misaka-palw/improve/hard-case/v1";
/// Key of a setter set's id.
pub const PALW_IMPROVE_SETTER_SET_DOMAIN_V1: &[u8] = b"misaka-palw/improve/setter-set/v1";
/// Key of a setter set's prompt commitment.
pub const PALW_IMPROVE_SETTER_PROMPTS_DOMAIN_V1: &[u8] = b"misaka-palw/improve/setter-prompts/v1";
/// Key of a setter set's key commitment.
pub const PALW_IMPROVE_SETTER_KEYS_DOMAIN_V1: &[u8] = b"misaka-palw/improve/setter-keys/v1";
/// Key of a registered dataset's id.
pub const PALW_IMPROVE_DATASET_DOMAIN_V1: &[u8] = b"misaka-palw/improve/dataset/v1";
/// Key of a teaching artifact's commitment (which is also its id).
pub const PALW_IMPROVE_ARTIFACT_DOMAIN_V1: &[u8] = b"misaka-palw/improve/teaching-artifact/v1";
/// Key of a teacher licence's id.
pub const PALW_IMPROVE_LICENCE_DOMAIN_V1: &[u8] = b"misaka-palw/improve/teacher-licence/v1";
/// Key of the message every signed A4 object's signer signs.
pub const PALW_IMPROVE_MATERIAL_MESSAGE_DOMAIN_V1: &[u8] = b"misaka-palw/improve/material/message/v1";
/// The ML-DSA-87 context of those signatures (spec 17 §17.0's spelling of a context).
pub const PALW_IMPROVE_MATERIAL_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw-improve-material-v1";
/// Key of a hard case's committed key or reference (`ExactKey`/`Continuation`'s commitment).
pub const PALW_IMPROVE_CASE_KEY_DOMAIN_V1: &[u8] = b"misaka-palw/improve/case-key/v1";

/// The longest prompt a case carries (and a setter prompt, a key, an answer span), in ids: J5b's
/// inline bound, so every case's prompt is one the chain can hash inline.
pub const PALW_IMPROVE_CASE_MAX_IDS_V1: usize = crate::palw_attempt_rules_v1::PALW_J5_INLINE_PROMPT_IDS_V1 as usize;
/// The most items one setter set may hold.
pub const PALW_IMPROVE_SETTER_SET_MAX_ITEMS_V1: u32 = 1 << 10;
/// The most ids a setter set's prompts hold together, and its keys: each reveal fits one carrier
/// (64 KB of ids under [`crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES`]; reveals do not chunk).
pub const PALW_IMPROVE_SETTER_REVEAL_MAX_IDS_V1: usize = 1 << 14;
/// The most licence classes a dataset may name, and domains a licence may cover.
pub const PALW_IMPROVE_MAX_CLASSES_V1: usize = 32;
/// An ML-DSA-87 public key's length.
pub const PALW_IMPROVE_MLDSA87_PUBKEY_BYTES_V1: usize = 2592;
/// The licence's use bit for training data (RFC-0004 §9: a licence for `LICENSED_DISTILL`).
pub const PALW_IMPROVE_LICENCE_USE_TRAINING_V1: u8 = 1;
/// The uses a licence may name in v1.
pub const PALW_IMPROVE_LICENCE_USES_V1: u8 = PALW_IMPROVE_LICENCE_USE_TRAINING_V1;

fn keyed(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for part in parts {
        state.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

fn bytes<T: BorshSerialize + ?Sized>(t: &T) -> Vec<u8> {
    borsh::to_vec(t).expect("a payload is borsh-serializable")
}

fn zero() -> Hash64 {
    Hash64::from_bytes([0; 64])
}

/// **A hard case's id**: its line, domain, prompt and reference — never its source or submitter, so
/// one case submitted twice is one case (the earlier admitted). Every part is fixed-width or borsh's
/// self-delimiting form, so no two cases share a preimage.
pub fn palw_hard_case_id_v1(line_id: &Hash64, domain: u16, prompt_ids: &[u32], reference: &PalwCaseReferenceV1) -> Hash64 {
    keyed(PALW_IMPROVE_CASE_DOMAIN_V1, &[line_id.as_byte_slice(), &domain.to_le_bytes(), &bytes(prompt_ids), &bytes(reference)])
}

/// **A case's key commitment** — what `ExactKey { commitment }` and `Continuation { commitment }`
/// hold: `H(line ‖ borsh(key) ‖ salt)`. Not over the case id, which is over the commitment.
pub fn palw_case_key_commitment_v1(line_id: &Hash64, key: &[u32], salt: &Hash64) -> Hash64 {
    keyed(PALW_IMPROVE_CASE_KEY_DOMAIN_V1, &[line_id.as_byte_slice(), &bytes(key), salt.as_byte_slice()])
}

/// **A setter set's prompt commitment**: `H(line ‖ le64 epoch ‖ borsh(prompts) ‖ salt)`.
pub fn palw_setter_prompts_commitment_v1(line_id: &Hash64, epoch: u64, prompts: &[Vec<u32>], salt: &Hash64) -> Hash64 {
    keyed(
        PALW_IMPROVE_SETTER_PROMPTS_DOMAIN_V1,
        &[line_id.as_byte_slice(), &epoch.to_le_bytes(), &bytes(prompts), salt.as_byte_slice()],
    )
}

/// **A setter set's key commitment**: `H(line ‖ le64 epoch ‖ borsh(keys) ‖ salt)` — the answer spans
/// and references, disclosed only after every subject's outputs are final.
pub fn palw_setter_keys_commitment_v1(line_id: &Hash64, epoch: u64, keys: &[Vec<u32>], salt: &Hash64) -> Hash64 {
    keyed(PALW_IMPROVE_SETTER_KEYS_DOMAIN_V1, &[line_id.as_byte_slice(), &epoch.to_le_bytes(), &bytes(keys), salt.as_byte_slice()])
}

/// **A setter set's id**: its line, epoch, item count and both commitments.
pub fn palw_setter_set_id_v1(c: &PalwSetterSetCommitmentV1) -> Hash64 {
    keyed(
        PALW_IMPROVE_SETTER_SET_DOMAIN_V1,
        &[
            c.line_id.as_byte_slice(),
            &c.epoch.to_le_bytes(),
            &c.items.to_le_bytes(),
            c.prompts_commitment.as_byte_slice(),
            c.keys_commitment.as_byte_slice(),
        ],
    )
}

/// **A registered dataset's id**: its line, content root, item count, classes and provenance.
pub fn palw_dataset_id_v1(d: &PalwDatasetV1) -> Hash64 {
    keyed(
        PALW_IMPROVE_DATASET_DOMAIN_V1,
        &[
            d.line_id.as_byte_slice(),
            d.content_root.as_byte_slice(),
            &d.items.to_le_bytes(),
            &bytes(&d.license_classes),
            &[d.teacher_classes],
            d.provenance_commitment.as_byte_slice(),
        ],
    )
}

/// **A teaching artifact's commitment** — `H(artifact ‖ salt)` over the revealed payload whole (its
/// salt inside it); also the artifact's id (a hard case's `Artifact { artifact_id }`).
pub fn palw_teaching_artifact_commit_v1(a: &PalwTeachingArtifactV1) -> Hash64 {
    keyed(PALW_IMPROVE_ARTIFACT_DOMAIN_V1, &[&bytes(a)])
}

/// **A teacher licence's id**: every term of it.
pub fn palw_teacher_licence_id_v1(l: &PalwTeacherLicenceV1) -> Hash64 {
    keyed(
        PALW_IMPROVE_LICENCE_DOMAIN_V1,
        &[
            &bytes(&l.rights_holder_key),
            l.model_family.as_byte_slice(),
            &bytes(&l.domains),
            &[l.uses],
            &l.per_use_fee.to_le_bytes(),
            &l.expiry_daa.to_le_bytes(),
        ],
    )
}

/// **The message an A4 object's signer signs**: the object's tag, the network domain, the payload
/// whole and the signing bond (none for a licence, which its rights holder's own key signs) — one
/// spelling for every signed material object, the tag keeping them apart, so a signature is neither
/// replayed on another network nor lifted onto another object or bond.
pub fn palw_improve_material_message_v1<T: BorshSerialize>(
    tag: u8,
    network_domain: &Hash64,
    payload: &T,
    signer: Option<&crate::palw_state_v2::PalwBondKeyV2>,
) -> Hash64 {
    let signer = signer.map(bytes).unwrap_or_default();
    keyed(PALW_IMPROVE_MATERIAL_MESSAGE_DOMAIN_V1, &[&[tag], network_domain.as_byte_slice(), &bytes(payload), &signer])
}

/// Why an A4 payload's form is refused (before any state is read).
pub type PalwMaterialFormV1 = Result<(), &'static str>;

fn ids_form(ids: &[u32], what: &'static str) -> PalwMaterialFormV1 {
    if ids.is_empty() || ids.len() > PALW_IMPROVE_CASE_MAX_IDS_V1 {
        return Err(what);
    }
    Ok(())
}

/// **A hard case's form** (RFC-0004 §5.1): a prompt of 1 to [`PALW_IMPROVE_CASE_MAX_IDS_V1`] ids, its
/// id derived, a committed reference never the zero hash, and hardness evidence only where a
/// reference can decide it (a judged-only case's hardness is J, never evidence). The ids' bound (the
/// line's tokenizer) and the source's and evidence's references are the fold's.
pub fn palw_hard_case_form_v1(c: &PalwHardCaseV1) -> PalwMaterialFormV1 {
    ids_form(&c.prompt_ids, "a hard case's prompt is empty or past the inline bound")?;
    if c.case_id != palw_hard_case_id_v1(&c.line_id, c.domain, &c.prompt_ids, &c.reference) {
        return Err("a hard case's id is not its derived id");
    }
    match c.reference {
        PalwCaseReferenceV1::ExactKey { commitment } | PalwCaseReferenceV1::Continuation { commitment } if commitment == zero() => {
            Err("a hard case's reference commits to nothing")
        }
        PalwCaseReferenceV1::None if c.head_evidence.is_some() => Err("a judged-only case carries hardness evidence"),
        _ => match c.source {
            PalwCaseSourceV1::UsageOptIn { job_pin } if job_pin == zero() => Err("a usage case names no job"),
            PalwCaseSourceV1::Artifact { artifact_id } if artifact_id == zero() => Err("an artifact case names no artifact"),
            _ => Ok(()),
        },
    }
}

/// **A case key's reveal form**: 1 to [`PALW_IMPROVE_CASE_MAX_IDS_V1`] ids. Its opening, the case's
/// reference kind, the epoch's disclosure window and the policy's `key_cap` are the fold's.
pub fn palw_case_key_reveal_form_v1(r: &PalwCaseKeyRevealV1) -> PalwMaterialFormV1 {
    ids_form(&r.key, "a case's key is empty or past the inline bound")
}

/// **A data-use opt-in's form** (RFC-0004 §10): the carried job facts hash to the pin (the claim's
/// recorded `job_identity` is the fold's), for a job with a prompt.
pub fn palw_data_use_opt_in_form_v1(o: &PalwDataUseOptInV1) -> PalwMaterialFormV1 {
    if o.job_pin == zero() || o.claim == zero() {
        return Err("an opt-in names no job or no claim");
    }
    if o.job.prompt_tokens == 0 || o.job.prompt_tokens as usize > PALW_IMPROVE_CASE_MAX_IDS_V1 {
        return Err("an opt-in for a job with no prompt, or one past the inline bound");
    }
    if o.job.pin() != o.job_pin {
        return Err("the carried job facts are not the pin's");
    }
    Ok(())
}

/// **A setter set's commitment form** (RFC-0004 §7.1): 1 to [`PALW_IMPROVE_SETTER_SET_MAX_ITEMS_V1`]
/// items, its id derived.
pub fn palw_setter_set_form_v1(c: &PalwSetterSetCommitmentV1) -> PalwMaterialFormV1 {
    if c.items == 0 || c.items > PALW_IMPROVE_SETTER_SET_MAX_ITEMS_V1 {
        return Err("a setter set of no items, or past the cap");
    }
    if c.set_id != palw_setter_set_id_v1(c) {
        return Err("a setter set's id is not its derived id");
    }
    Ok(())
}

fn reveal_of(c: &PalwSetterSetCommitmentV1, line_id: &Hash64, epoch: u64, set_id: &Hash64) -> PalwMaterialFormV1 {
    if (*line_id, epoch, *set_id) != (c.line_id, c.epoch, c.set_id) {
        return Err("a reveal of another set");
    }
    Ok(())
}

/// **A setter set's prompts open its commitment** (RFC-0004 §7.1): one prompt per committed item,
/// each of 1 to [`PALW_IMPROVE_CASE_MAX_IDS_V1`] ids and [`PALW_IMPROVE_SETTER_REVEAL_MAX_IDS_V1`]
/// together, hashing with the salt to the commitment.
pub fn palw_setter_prompts_open_v1(c: &PalwSetterSetCommitmentV1, r: &PalwSetterSetRevealV1) -> PalwMaterialFormV1 {
    reveal_of(c, &r.line_id, r.epoch, &r.set_id)?;
    if r.prompts.len() != c.items as usize {
        return Err("not one prompt per committed item");
    }
    for p in &r.prompts {
        ids_form(p, "a setter prompt is empty or past the inline bound")?;
    }
    if r.prompts.iter().map(Vec::len).sum::<usize>() > PALW_IMPROVE_SETTER_REVEAL_MAX_IDS_V1 {
        return Err("a setter set's prompts are past the reveal's cap");
    }
    if palw_setter_prompts_commitment_v1(&c.line_id, c.epoch, &r.prompts, &r.salt) != c.prompts_commitment {
        return Err("the prompts do not open the set's commitment");
    }
    Ok(())
}

/// **A setter set's keys open its commitment**: one key per committed item — empty for a judged item,
/// which has none — each within the inline bound and [`PALW_IMPROVE_SETTER_REVEAL_MAX_IDS_V1`]
/// together, hashing with the salt to the commitment.
pub fn palw_setter_keys_open_v1(c: &PalwSetterSetCommitmentV1, r: &PalwSetterKeysRevealV1) -> PalwMaterialFormV1 {
    reveal_of(c, &r.line_id, r.epoch, &r.set_id)?;
    if r.keys.len() != c.items as usize {
        return Err("not one key per committed item");
    }
    if r.keys.iter().any(|k| k.len() > PALW_IMPROVE_CASE_MAX_IDS_V1) {
        return Err("a setter key past the inline bound");
    }
    if r.keys.iter().map(Vec::len).sum::<usize>() > PALW_IMPROVE_SETTER_REVEAL_MAX_IDS_V1 {
        return Err("a setter set's keys are past the reveal's cap");
    }
    if palw_setter_keys_commitment_v1(&c.line_id, c.epoch, &r.keys, &r.salt) != c.keys_commitment {
        return Err("the keys do not open the set's commitment");
    }
    Ok(())
}

/// The teacher classes the protocol defines, as a mask.
pub fn palw_teacher_classes_defined_v1() -> u8 {
    PalwTeacherClassV1::ALL.iter().fold(0u8, |m, c| m | c.bit())
}

/// **A dataset's form** (RFC-0004 §5.3): at least one item, 1 to [`PALW_IMPROVE_MAX_CLASSES_V1`]
/// licence classes, sorted and distinct, teacher classes defined and at least one, its id derived.
/// Whether the policy allows its classes is the fold's.
pub fn palw_dataset_form_v1(d: &PalwDatasetV1) -> PalwMaterialFormV1 {
    if d.items == 0 {
        return Err("a dataset of no items");
    }
    if d.license_classes.is_empty() || d.license_classes.len() > PALW_IMPROVE_MAX_CLASSES_V1 {
        return Err("a dataset names no licence class, or too many");
    }
    if d.license_classes.windows(2).any(|w| w[0] >= w[1]) {
        return Err("a dataset's licence classes are not sorted and distinct");
    }
    if d.teacher_classes == 0 || d.teacher_classes & !palw_teacher_classes_defined_v1() != 0 {
        return Err("a dataset's teacher classes are none, or not the protocol's");
    }
    if d.dataset_id != palw_dataset_id_v1(d) {
        return Err("a dataset's id is not its derived id");
    }
    Ok(())
}

/// **Which verification types a kind admits in Phase A** (RFC-0004 §5.3's table): an `Answer` is
/// EXACT against its case's key; a `PreferencePair` EXACT (one outcome matches the key and one does
/// not), JUDGED or HUMAN; a `SyntheticProblem` and a `HardCaseVariant` are paid only where the head
/// verifiably fails them, so EXACT or LIKELIHOOD; a `RewardSignal` is a `SELF_PLAY` rollout's EXACT
/// or LIKELIHOOD score; a `Critique` is JUDGED only (an executed counterexample needs Phase C). A
/// `TOOL_VERIFIED` artifact's outcome is verified by EXACT, whatever its kind.
pub fn palw_teaching_artifact_kind_admits_v1(
    kind: PalwTeachingArtifactKindV1,
    verification: PalwVerificationTypeV1,
    teacher: PalwTeacherClassV1,
) -> bool {
    use PalwTeachingArtifactKindV1 as K;
    use PalwVerificationTypeV1 as V;
    let by_kind = match kind {
        K::Answer => verification == V::Exact,
        K::PreferencePair => matches!(verification, V::Exact | V::Judged | V::Human),
        K::SyntheticProblem | K::HardCaseVariant => matches!(verification, V::Exact | V::Likelihood),
        K::RewardSignal => teacher == PalwTeacherClassV1::SelfPlay && matches!(verification, V::Exact | V::Likelihood),
        K::Critique => verification == V::Judged,
    };
    by_kind && (teacher != PalwTeacherClassV1::ToolVerified || verification == V::Exact)
}

/// **A revealed teaching artifact's form** (RFC-0004 §5.3): a kind its verification type verifies in
/// Phase A ([`palw_teaching_artifact_kind_admits_v1`]), an answer span exactly on an `Answer` (within
/// the inline bound), content hashed, and a licence class named. A reveal that opens its commitment
/// but fails this is spam: its bond is forfeited, never refused (the fold's).
pub fn palw_teaching_artifact_form_v1(a: &PalwTeachingArtifactV1) -> PalwMaterialFormV1 {
    if !palw_teaching_artifact_kind_admits_v1(a.kind, a.verification_type, a.teacher_type) {
        return Err("a kind its verification type does not verify in Phase A");
    }
    if (a.kind == PalwTeachingArtifactKindV1::Answer) == a.answer_span.is_empty() {
        return Err("an answer span rides exactly with an Answer");
    }
    if a.answer_span.len() > PALW_IMPROVE_CASE_MAX_IDS_V1 {
        return Err("an answer span past the inline bound");
    }
    if a.output_hash == zero() || a.task_id == zero() || a.license_class == zero() {
        return Err("an artifact hashes no content, names no task or no licence class");
    }
    Ok(())
}

/// **A teacher licence's form** (RFC-0004 §9): an ML-DSA-87 key, the training use (the only use v1
/// defines), at most [`PALW_IMPROVE_MAX_CLASSES_V1`] sorted, distinct domains, a model family, its id
/// derived. Its expiry against the chain's height is the fold's.
pub fn palw_teacher_licence_form_v1(l: &PalwTeacherLicenceV1) -> PalwMaterialFormV1 {
    if l.rights_holder_key.len() != PALW_IMPROVE_MLDSA87_PUBKEY_BYTES_V1 {
        return Err("a licence's rights holder key is not an ML-DSA-87 key");
    }
    if l.uses & PALW_IMPROVE_LICENCE_USE_TRAINING_V1 == 0 || l.uses & !PALW_IMPROVE_LICENCE_USES_V1 != 0 {
        return Err("a teacher licence that does not license training data, or names an undefined use");
    }
    if l.domains.len() > PALW_IMPROVE_MAX_CLASSES_V1 || l.domains.windows(2).any(|w| w[0] >= w[1]) {
        return Err("a licence's domains are past the cap, or not sorted and distinct");
    }
    if l.model_family == zero() {
        return Err("a licence names no model family");
    }
    if l.licence_id != palw_teacher_licence_id_v1(l) {
        return Err("a licence's id is not its derived id");
    }
    Ok(())
}

/// **Does a licence cover an artifact's use** (RFC-0004 §9): unexpired at `daa`, for training data,
/// and — when it names domains — for the artifact's domain. An empty domain list covers every domain.
pub fn palw_teacher_licence_covers_v1(l: &PalwTeacherLicenceV1, domain: u16, daa: u64) -> bool {
    daa < l.expiry_daa
        && l.uses & PALW_IMPROVE_LICENCE_USE_TRAINING_V1 != 0
        && (l.domains.is_empty() || l.domains.binary_search(&domain).is_ok())
}

// ---- the records A4's tables keep (the root block `improvement-material/v1`) ----

/// **A hard case as the chain keeps it**, by `(line, case id)`: the case, who submitted it and when,
/// the epoch whose material or hold-out pool took it (the core's placement), and — once opened — its
/// committed key or reference. It retires with that epoch.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwHardCaseRecordV1 {
    pub case: PalwHardCaseV1,
    pub submitter: crate::palw_state_v2::PalwBondKeyV2,
    pub admitted_daa: u64,
    pub epoch: u64,
    /// In the epoch's hold-out pool (else its training material).
    pub holdout: bool,
    /// The key or reference `HardCaseKeyRevealed` opened.
    pub revealed: Option<Vec<u32>>,
}

/// **A job's opt-in as the chain keeps it**, by job pin: the claim, its committer, the job's prompt
/// commitment and its form, the tokenizer and prompt length, when — until the claim's retention ends.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwDataUseOptInRecordV1 {
    pub claim: Hash64,
    pub committer: crate::palw_state_v2::PalwBondKeyV2,
    pub prompt_token_ids_hash: Hash64,
    /// Whether the commitment is the tiled Merkle root (`true`) or the flat digest — the claim's
    /// class's form ([`crate::palw_prompt_ids_v1::palw_prompt_ids_form_of_class_v1`]).
    pub prompt_ids_merkle: bool,
    pub tokenizer_id: Hash64,
    pub prompt_tokens: u32,
    pub opted_in_daa: u64,
    /// The claim's `trace_retention_daa`: the opt-in leaves when the claim's evidence does.
    pub expires_daa: u64,
}

impl PalwDataUseOptInRecordV1 {
    /// **Is `ids` the opted-in job's prompt** — the one comparison every reader of a
    /// `prompt_token_ids_hash` makes ([`crate::palw_prompt_ids_v1::prompt_token_ids_match_v1`]), under
    /// the form the job committed in, at the committed length.
    pub fn prompt_is(&self, ids: &[u32]) -> bool {
        use crate::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_match_v1};
        let form = if self.prompt_ids_merkle { PalwPromptIdsFormV1::MerkleV1 } else { PalwPromptIdsFormV1::Flat };
        ids.len() == self.prompt_tokens as usize && prompt_token_ids_match_v1(form, ids, &self.prompt_token_ids_hash)
    }
}

/// **A setter set as the chain keeps it**, by `(line, set id)`: its commitment (the epoch in it), its
/// steward, when, the bond held, and what it has revealed. It retires with its epoch — refunded if it
/// owed no reveal it missed; one that missed a reveal forfeits and leaves right before scoring.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSetterSetRecordV1 {
    pub commitment: PalwSetterSetCommitmentV1,
    pub setter: crate::palw_state_v2::PalwBondKeyV2,
    pub committed_daa: u64,
    pub bond: u64,
    pub prompts: Option<Vec<Vec<u32>>>,
    pub keys: Option<Vec<Vec<u32>>>,
}

/// **A registered dataset as the chain keeps it**, by `(line, dataset id)`: the bond it holds while it
/// stands (v1 has no deregistration; a line leaving governance refunds it), and the epoch whose
/// material took it.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwDatasetRecordV1 {
    pub dataset: PalwDatasetV1,
    pub registrant: crate::palw_state_v2::PalwBondKeyV2,
    pub registered_daa: u64,
    pub bond: u64,
    pub epoch: u64,
}

/// **A teaching artifact as the chain keeps it**, by `(line, commitment)`: its teacher, when it was
/// committed (commit order decides duplicates and bounties), the bond held, its reveal deadline, and —
/// once revealed — the artifact and the epoch whose material took it. Spam and duplicates forfeit and
/// leave when found; an honest artifact is refunded when its epoch retires.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTeachingArtifactRecordV1 {
    pub commit: PalwTeachingArtifactCommitV1,
    pub teacher: crate::palw_state_v2::PalwBondKeyV2,
    pub committed_daa: u64,
    pub bond: u64,
    /// `committed_daa + w_collect` of the policy at the commit: unrevealed then, it forfeits.
    pub reveal_by_daa: u64,
    pub revealed: Option<PalwTeachingArtifactV1>,
    pub revealed_daa: u64,
    pub epoch: u64,
}

/// **A teacher licence as the chain keeps it**, by licence id, until it expires.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTeacherLicenceRecordV1 {
    pub licence: PalwTeacherLicenceV1,
    pub registered_daa: u64,
}

/// **Test rows** for the state module's root and carriage suites: one of each material table's rows,
/// varied by `seed`, with every field populated.
#[cfg(test)]
pub(crate) mod test_rows {
    use super::*;
    use crate::palw_state_v2::PalwBondKeyV2;
    use crate::tx::TransactionOutpoint;

    fn h(seed: u8, lane: u8) -> Hash64 {
        let mut bytes = [seed; 64];
        bytes[0] = lane;
        Hash64::from_bytes(bytes)
    }

    fn bond(seed: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_bytes([seed; 64]), index: seed as u32 })
    }

    pub(crate) fn case_v1(seed: u8) -> PalwHardCaseRecordV1 {
        let (line_id, prompt_ids, reference) =
            (h(seed, 1), vec![seed as u32, 2], PalwCaseReferenceV1::ExactKey { commitment: h(seed, 2) });
        PalwHardCaseRecordV1 {
            case: PalwHardCaseV1 {
                line_id,
                case_id: palw_hard_case_id_v1(&line_id, 7, &prompt_ids, &reference),
                domain: 7,
                prompt_ids,
                reference,
                source: PalwCaseSourceV1::Setter,
                head_evidence: Some(h(seed, 3)),
            },
            submitter: bond(seed),
            admitted_daa: seed as u64,
            epoch: 2,
            holdout: true,
            revealed: Some(vec![9]),
        }
    }

    pub(crate) fn opt_in_v1(seed: u8) -> PalwDataUseOptInRecordV1 {
        PalwDataUseOptInRecordV1 {
            claim: h(seed, 4),
            committer: bond(seed),
            prompt_token_ids_hash: h(seed, 5),
            prompt_ids_merkle: true,
            tokenizer_id: h(seed, 6),
            prompt_tokens: 3,
            opted_in_daa: seed as u64,
            expires_daa: 1_000 + seed as u64,
        }
    }

    pub(crate) fn setter_set_v1(seed: u8) -> PalwSetterSetRecordV1 {
        let mut commitment = PalwSetterSetCommitmentV1 {
            line_id: h(seed, 7),
            epoch: 3,
            set_id: Hash64::default(),
            items: 2,
            prompts_commitment: h(seed, 8),
            keys_commitment: h(seed, 9),
        };
        commitment.set_id = palw_setter_set_id_v1(&commitment);
        PalwSetterSetRecordV1 {
            commitment,
            setter: bond(seed),
            committed_daa: seed as u64,
            bond: 5,
            prompts: Some(vec![vec![1], vec![2]]),
            keys: Some(vec![vec![3], vec![]]),
        }
    }

    pub(crate) fn dataset_v1(seed: u8) -> PalwDatasetRecordV1 {
        let mut dataset = PalwDatasetV1 {
            line_id: h(seed, 10),
            dataset_id: Hash64::default(),
            content_root: h(seed, 11),
            items: 4,
            license_classes: vec![h(seed, 12)],
            teacher_classes: 1,
            provenance_commitment: h(seed, 13),
        };
        dataset.dataset_id = palw_dataset_id_v1(&dataset);
        PalwDatasetRecordV1 { dataset, registrant: bond(seed), registered_daa: seed as u64, bond: 6, epoch: 1 }
    }

    pub(crate) fn artifact_v1(seed: u8) -> PalwTeachingArtifactRecordV1 {
        let revealed = PalwTeachingArtifactV1 {
            line_id: h(seed, 14),
            kind: PalwTeachingArtifactKindV1::Answer,
            task_id: h(seed, 15),
            teacher_type: PalwTeacherClassV1::Human,
            teacher_id: h(seed, 16),
            license_class: h(seed, 17),
            provenance_commitment: h(seed, 18),
            output_hash: h(seed, 19),
            verification_type: PalwVerificationTypeV1::Exact,
            answer_span: vec![4],
            salt: h(seed, 20),
        };
        PalwTeachingArtifactRecordV1 {
            commit: PalwTeachingArtifactCommitV1 { line_id: revealed.line_id, commit: palw_teaching_artifact_commit_v1(&revealed) },
            teacher: bond(seed),
            committed_daa: seed as u64,
            bond: 7,
            reveal_by_daa: 100 + seed as u64,
            revealed_daa: 50,
            epoch: 1,
            revealed: Some(revealed),
        }
    }

    pub(crate) fn licence_v1(seed: u8) -> PalwTeacherLicenceRecordV1 {
        let mut licence = PalwTeacherLicenceV1 {
            licence_id: Hash64::default(),
            rights_holder_key: vec![seed; PALW_IMPROVE_MLDSA87_PUBKEY_BYTES_V1],
            model_family: h(seed, 21),
            domains: vec![1, 2],
            uses: PALW_IMPROVE_LICENCE_USE_TRAINING_V1,
            per_use_fee: 8,
            expiry_daa: 10_000,
        };
        licence.licence_id = palw_teacher_licence_id_v1(&licence);
        PalwTeacherLicenceRecordV1 { licence, registered_daa: seed as u64 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_improve_state_v1::{PalwTeacherClassV1 as T, PalwTeachingArtifactKindV1 as K, PalwVerificationTypeV1 as V};
    use crate::palw_state_v2::PalwBondKeyV2;
    use crate::tx::TransactionOutpoint;

    fn h(word: u64) -> Hash64 {
        Hash64::from_u64_word(word)
    }

    fn bond(seed: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_bytes([seed; 64]), 0))
    }

    fn case() -> PalwHardCaseV1 {
        let (line_id, domain, prompt_ids, reference) = (h(1), 7, vec![5, 6, 7], PalwCaseReferenceV1::ExactKey { commitment: h(9) });
        PalwHardCaseV1 {
            line_id,
            case_id: palw_hard_case_id_v1(&line_id, domain, &prompt_ids, &reference),
            domain,
            prompt_ids,
            reference,
            source: PalwCaseSourceV1::Setter,
            head_evidence: Some(h(4)),
        }
    }

    fn rederive(mut c: PalwHardCaseV1) -> PalwHardCaseV1 {
        c.case_id = palw_hard_case_id_v1(&c.line_id, c.domain, &c.prompt_ids, &c.reference);
        c
    }

    #[test]
    fn a_hard_case_is_named_by_what_it_asks_never_by_who_asks() {
        let c = case();
        assert_eq!(palw_hard_case_form_v1(&c), Ok(()));
        let resubmitted =
            PalwHardCaseV1 { source: PalwCaseSourceV1::Artifact { artifact_id: h(3) }, head_evidence: None, ..c.clone() };
        assert_eq!(resubmitted.case_id, c.case_id, "the source and the evidence are not in the id");
        for (why, bad) in [
            ("empty", rederive(PalwHardCaseV1 { prompt_ids: vec![], ..c.clone() })),
            ("past the inline bound", rederive(PalwHardCaseV1 { prompt_ids: vec![1; PALW_IMPROVE_CASE_MAX_IDS_V1 + 1], ..c.clone() })),
            ("another id", PalwHardCaseV1 { case_id: h(2), ..c.clone() }),
            (
                "a reference to nothing",
                rederive(PalwHardCaseV1 { reference: PalwCaseReferenceV1::Continuation { commitment: zero() }, ..c.clone() }),
            ),
            ("judged-only with evidence", rederive(PalwHardCaseV1 { reference: PalwCaseReferenceV1::None, ..c.clone() })),
            ("a usage case of no job", PalwHardCaseV1 { source: PalwCaseSourceV1::UsageOptIn { job_pin: zero() }, ..c.clone() }),
            (
                "an artifact case of no artifact",
                PalwHardCaseV1 { source: PalwCaseSourceV1::Artifact { artifact_id: zero() }, ..c.clone() },
            ),
        ] {
            assert!(palw_hard_case_form_v1(&bad).is_err(), "{why}");
        }
        let judged = rederive(PalwHardCaseV1 { reference: PalwCaseReferenceV1::None, head_evidence: None, ..c.clone() });
        assert_eq!(palw_hard_case_form_v1(&judged), Ok(()), "a judged-only case without evidence");
        assert_ne!(judged.case_id, c.case_id, "the reference is in the id");
        let longest = rederive(PalwHardCaseV1 { prompt_ids: vec![1; PALW_IMPROVE_CASE_MAX_IDS_V1], ..c });
        assert_eq!(palw_hard_case_form_v1(&longest), Ok(()), "the inline bound itself");
    }

    fn fp_commitment() -> crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3 {
        use crate::palw_freeprompt_v3 as fp;
        fp::PalwFreePromptCommitmentV3 {
            job: fp::PalwFreePromptJobV3 {
                version: fp::PALW_FP_V3_VERSION,
                network_domain: h(0x4E45_5457),
                class_id: h(0xC1),
                executor_bond: TransactionOutpoint::new(Hash64::from_bytes([1; 64]), 0),
                executor_pubkey: vec![7u8; 32],
                operator_id: h(0xE0),
                anchor_block: h(0xA0),
                anchor_daa: 5_000,
                job_nonce: [0x11; 32],
                tokenizer_id: h(0x70),
                prompt_token_ids_hash: crate::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(
                    crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                    &[5, 6, 7],
                )
                .unwrap(),
                prompt_tokens: 3,
                decode_token_limit: 128,
                max_context_tokens: 4096,
                privacy_mode: fp::PALW_FP_PRIVACY_PUBLIC_DA,
                prompt_mode: fp::PALW_FP_PROMPT_MODE_USER,
                sampling_seed: crate::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
                temperature_q: crate::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
                decode: None,
                tail: None,
            },
            trace_root: h(0x7A),
            output_root: h(0),
            schedule_root: h(0x5C),
            execution_root: h(0x4E),
            decode_tokens_executed: 77,
            stop_reason: fp::PalwFpStopReasonV3::EndOfGeneration,
            work_leaves: 4_096,
            trace_manifest_root: h(0xD0),
            trace_chunk_count: 8,
            trace_retention_daa: 999_999,
        }
    }

    #[test]
    fn an_opt_in_carries_the_facts_the_claims_pin_hashes() {
        let c = fp_commitment();
        let job = PalwFpJobFactsV1::of_commitment(&c);
        let pin = crate::palw_fp_execution_v3::palw_fp_job_pin_v1(&c);
        assert_eq!(job.pin(), pin, "one spelling of the pin: the claim path's");
        let o = PalwDataUseOptInV1 { job_pin: pin, claim: h(0xC0), job };
        assert_eq!(palw_data_use_opt_in_form_v1(&o), Ok(()));
        for (why, bad) in [
            ("another job's facts", PalwDataUseOptInV1 { job: PalwFpJobFactsV1 { decode_tokens_executed: 78, ..job }, ..o.clone() }),
            ("another pin", PalwDataUseOptInV1 { job_pin: h(5), ..o.clone() }),
            ("no claim", PalwDataUseOptInV1 { claim: zero(), ..o.clone() }),
            (
                "no prompt",
                PalwDataUseOptInV1 {
                    job: PalwFpJobFactsV1 { prompt_tokens: 0, ..job },
                    job_pin: PalwFpJobFactsV1 { prompt_tokens: 0, ..job }.pin(),
                    ..o.clone()
                },
            ),
        ] {
            assert!(palw_data_use_opt_in_form_v1(&bad).is_err(), "{why}");
        }
        // The case's prompt is the job's exactly when it hashes to the committed prompt, at its length,
        // in the form the job committed in.
        let record = PalwDataUseOptInRecordV1 {
            claim: o.claim,
            committer: bond(1),
            prompt_token_ids_hash: job.prompt_token_ids_hash,
            prompt_ids_merkle: false,
            tokenizer_id: job.tokenizer_id,
            prompt_tokens: job.prompt_tokens,
            opted_in_daa: 1,
            expires_daa: 100,
        };
        assert!(record.prompt_is(&[5, 6, 7]));
        assert!(!record.prompt_is(&[5, 6, 8]) && !record.prompt_is(&[5, 6]));
        assert!(
            !PalwDataUseOptInRecordV1 { prompt_ids_merkle: true, ..record.clone() }.prompt_is(&[5, 6, 7]),
            "the form is the job's"
        );
        let merkle = crate::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(
            crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
            &[5, 6, 7],
        )
        .unwrap();
        assert!(PalwDataUseOptInRecordV1 { prompt_ids_merkle: true, prompt_token_ids_hash: merkle, ..record }.prompt_is(&[5, 6, 7]));
    }

    fn set(items: u32, prompts: &[Vec<u32>], keys: &[Vec<u32>]) -> PalwSetterSetCommitmentV1 {
        let (line_id, epoch) = (h(1), 3);
        let mut c = PalwSetterSetCommitmentV1 {
            line_id,
            epoch,
            set_id: zero(),
            items,
            prompts_commitment: palw_setter_prompts_commitment_v1(&line_id, epoch, prompts, &h(0x5A)),
            keys_commitment: palw_setter_keys_commitment_v1(&line_id, epoch, keys, &h(0x5B)),
        };
        c.set_id = palw_setter_set_id_v1(&c);
        c
    }

    #[test]
    fn a_setter_set_opens_only_to_what_it_committed() {
        let prompts = vec![vec![1, 2], vec![3]];
        let keys = vec![vec![9], vec![]];
        let c = set(2, &prompts, &keys);
        assert_eq!(palw_setter_set_form_v1(&c), Ok(()));
        assert!(palw_setter_set_form_v1(&PalwSetterSetCommitmentV1 { items: 3, ..c }).is_err(), "the id is over the count");
        assert!(palw_setter_set_form_v1(&set(0, &[], &[])).is_err(), "no items");
        assert!(palw_setter_set_form_v1(&set(PALW_IMPROVE_SETTER_SET_MAX_ITEMS_V1 + 1, &prompts, &keys)).is_err(), "past the cap");
        let reveal =
            PalwSetterSetRevealV1 { line_id: c.line_id, epoch: c.epoch, set_id: c.set_id, prompts: prompts.clone(), salt: h(0x5A) };
        assert_eq!(palw_setter_prompts_open_v1(&c, &reveal), Ok(()));
        let keys_reveal =
            PalwSetterKeysRevealV1 { line_id: c.line_id, epoch: c.epoch, set_id: c.set_id, keys: keys.clone(), salt: h(0x5B) };
        assert_eq!(palw_setter_keys_open_v1(&c, &keys_reveal), Ok(()), "a judged item's key is empty");
        for (why, bad) in [
            ("another salt", PalwSetterSetRevealV1 { salt: h(0x5B), ..reveal.clone() }),
            ("another prompt", PalwSetterSetRevealV1 { prompts: vec![vec![1, 2], vec![4]], ..reveal.clone() }),
            ("a prompt short", PalwSetterSetRevealV1 { prompts: vec![vec![1, 2]], ..reveal.clone() }),
            ("another set", PalwSetterSetRevealV1 { set_id: h(8), ..reveal.clone() }),
            ("another epoch", PalwSetterSetRevealV1 { epoch: 4, ..reveal.clone() }),
        ] {
            assert!(palw_setter_prompts_open_v1(&c, &bad).is_err(), "{why}");
        }
        assert!(
            palw_setter_keys_open_v1(&c, &PalwSetterKeysRevealV1 { keys: vec![vec![9], vec![1]], ..keys_reveal.clone() }).is_err()
        );
        assert!(
            palw_setter_keys_open_v1(&c, &PalwSetterKeysRevealV1 { salt: h(0x5A), ..keys_reveal }).is_err(),
            "each commitment its salt"
        );
        // An empty prompt, and a set whose prompts together pass the carrier-sized cap, are refused even
        // when they open the commitment.
        let empty = vec![vec![], vec![3]];
        let c = set(2, &empty, &keys);
        let reveal = PalwSetterSetRevealV1 { line_id: c.line_id, epoch: c.epoch, set_id: c.set_id, prompts: empty, salt: h(0x5A) };
        assert!(palw_setter_prompts_open_v1(&c, &reveal).is_err(), "an empty prompt");
        let wide =
            vec![vec![1; PALW_IMPROVE_CASE_MAX_IDS_V1]; PALW_IMPROVE_SETTER_REVEAL_MAX_IDS_V1 / PALW_IMPROVE_CASE_MAX_IDS_V1 + 1];
        let c = set(wide.len() as u32, &wide, &vec![vec![]; wide.len()]);
        let reveal = PalwSetterSetRevealV1 { line_id: c.line_id, epoch: c.epoch, set_id: c.set_id, prompts: wide, salt: h(0x5A) };
        assert!(palw_setter_prompts_open_v1(&c, &reveal).is_err(), "past the reveal's cap");
    }

    fn dataset() -> PalwDatasetV1 {
        let mut d = PalwDatasetV1 {
            line_id: h(1),
            dataset_id: zero(),
            content_root: h(2),
            items: 10,
            license_classes: vec![h(3), h(4)],
            teacher_classes: T::Human.bit() | T::PublicData.bit(),
            provenance_commitment: h(5),
        };
        d.dataset_id = palw_dataset_id_v1(&d);
        d
    }

    #[test]
    fn a_dataset_declares_sorted_classes_the_protocol_defines() {
        let d = dataset();
        assert_eq!(palw_dataset_form_v1(&d), Ok(()));
        let rederived = |mut d: PalwDatasetV1| {
            d.dataset_id = palw_dataset_id_v1(&d);
            d
        };
        for (why, bad) in [
            ("no items", rederived(PalwDatasetV1 { items: 0, ..d.clone() })),
            ("no licence class", rederived(PalwDatasetV1 { license_classes: vec![], ..d.clone() })),
            ("unsorted", rederived(PalwDatasetV1 { license_classes: vec![h(4), h(3)], ..d.clone() })),
            ("twice", rederived(PalwDatasetV1 { license_classes: vec![h(3), h(3)], ..d.clone() })),
            ("too many", rederived(PalwDatasetV1 { license_classes: (0..33).map(h).collect(), ..d.clone() })),
            ("no teacher class", rederived(PalwDatasetV1 { teacher_classes: 0, ..d.clone() })),
            ("an undefined teacher class", rederived(PalwDatasetV1 { teacher_classes: 0x40, ..d.clone() })),
            ("another id", PalwDatasetV1 { dataset_id: h(9), ..d.clone() }),
        ] {
            assert!(palw_dataset_form_v1(&bad).is_err(), "{why}");
        }
        assert_eq!(palw_teacher_classes_defined_v1(), 0x3F);
    }

    fn artifact(kind: K, verification_type: V, teacher_type: T) -> PalwTeachingArtifactV1 {
        PalwTeachingArtifactV1 {
            line_id: h(1),
            kind,
            task_id: h(2),
            teacher_type,
            teacher_id: h(3),
            license_class: h(4),
            provenance_commitment: h(5),
            output_hash: h(6),
            verification_type,
            answer_span: if kind == K::Answer { vec![42] } else { vec![] },
            salt: h(7),
        }
    }

    #[test]
    fn an_artifact_is_verified_the_way_section_5_3_says_its_kind_is() {
        let kinds = [K::Answer, K::PreferencePair, K::SyntheticProblem, K::HardCaseVariant, K::RewardSignal, K::Critique];
        let types = [V::Exact, V::Likelihood, V::Judged, V::Human];
        let expected = |k: K, v: V, t: T| -> bool {
            let by_kind = match k {
                K::Answer => v == V::Exact,
                K::PreferencePair => v != V::Likelihood,
                K::SyntheticProblem | K::HardCaseVariant => matches!(v, V::Exact | V::Likelihood),
                K::RewardSignal => t == T::SelfPlay && matches!(v, V::Exact | V::Likelihood),
                K::Critique => v == V::Judged,
            };
            by_kind && (t != T::ToolVerified || v == V::Exact)
        };
        let mut admitted = 0;
        for k in kinds {
            for v in types {
                for t in T::ALL {
                    let a = artifact(k, v, t);
                    assert_eq!(palw_teaching_artifact_kind_admits_v1(k, v, t), expected(k, v, t), "{k:?} {v:?} {t:?}");
                    assert_eq!(palw_teaching_artifact_form_v1(&a).is_ok(), expected(k, v, t), "{k:?} {v:?} {t:?}");
                    admitted += expected(k, v, t) as usize;
                }
            }
        }
        // Answer 6; PreferencePair 6 + 5 + 5 (a TOOL_VERIFIED pair is EXACT only); SyntheticProblem and
        // HardCaseVariant 6 + 5 each; RewardSignal 2 (SELF_PLAY); Critique 5.
        assert_eq!(admitted, 6 + 16 + 11 + 11 + 2 + 5, "the table's admitted cells");
        let answer = artifact(K::Answer, V::Exact, T::Human);
        for (why, bad) in [
            ("an Answer without its span", PalwTeachingArtifactV1 { answer_span: vec![], ..answer.clone() }),
            (
                "a span past the bound",
                PalwTeachingArtifactV1 { answer_span: vec![1; PALW_IMPROVE_CASE_MAX_IDS_V1 + 1], ..answer.clone() },
            ),
            ("no content", PalwTeachingArtifactV1 { output_hash: zero(), ..answer.clone() }),
            ("no task", PalwTeachingArtifactV1 { task_id: zero(), ..answer.clone() }),
            ("no licence class", PalwTeachingArtifactV1 { license_class: zero(), ..answer.clone() }),
            (
                "a span on another kind",
                PalwTeachingArtifactV1 { answer_span: vec![1], ..artifact(K::PreferencePair, V::Exact, T::Human) },
            ),
        ] {
            assert!(palw_teaching_artifact_form_v1(&bad).is_err(), "{why}");
        }
        // The commitment is over the payload whole, its salt inside it.
        assert_ne!(
            palw_teaching_artifact_commit_v1(&answer),
            palw_teaching_artifact_commit_v1(&PalwTeachingArtifactV1 { salt: h(8), ..answer.clone() })
        );
    }

    fn licence() -> PalwTeacherLicenceV1 {
        let mut l = PalwTeacherLicenceV1 {
            licence_id: zero(),
            rights_holder_key: vec![3; PALW_IMPROVE_MLDSA87_PUBKEY_BYTES_V1],
            model_family: h(1),
            domains: vec![2, 5],
            uses: PALW_IMPROVE_LICENCE_USE_TRAINING_V1,
            per_use_fee: 10,
            expiry_daa: 1_000,
        };
        l.licence_id = palw_teacher_licence_id_v1(&l);
        l
    }

    #[test]
    fn a_licence_covers_training_in_its_domains_until_it_expires() {
        let l = licence();
        assert_eq!(palw_teacher_licence_form_v1(&l), Ok(()));
        let rederived = |mut l: PalwTeacherLicenceV1| {
            l.licence_id = palw_teacher_licence_id_v1(&l);
            l
        };
        for (why, bad) in [
            ("not an ML-DSA-87 key", rederived(PalwTeacherLicenceV1 { rights_holder_key: vec![3; 32], ..l.clone() })),
            ("no training use", rederived(PalwTeacherLicenceV1 { uses: 0, ..l.clone() })),
            ("an undefined use", rederived(PalwTeacherLicenceV1 { uses: 3, ..l.clone() })),
            ("unsorted domains", rederived(PalwTeacherLicenceV1 { domains: vec![5, 2], ..l.clone() })),
            ("no model family", rederived(PalwTeacherLicenceV1 { model_family: zero(), ..l.clone() })),
            ("another id", PalwTeacherLicenceV1 { licence_id: h(9), ..l.clone() }),
        ] {
            assert!(palw_teacher_licence_form_v1(&bad).is_err(), "{why}");
        }
        assert!(palw_teacher_licence_covers_v1(&l, 5, 999));
        assert!(!palw_teacher_licence_covers_v1(&l, 5, 1_000), "expired at its expiry");
        assert!(!palw_teacher_licence_covers_v1(&l, 3, 1), "outside its domains");
        assert!(palw_teacher_licence_covers_v1(&PalwTeacherLicenceV1 { domains: vec![], ..l }, 3, 1), "no domains: every domain");
    }

    #[test]
    fn a_signed_material_message_binds_its_tag_network_payload_and_signer() {
        let c = case();
        let m = palw_improve_material_message_v1(71, &h(1), &c, Some(&bond(1)));
        assert_ne!(m, palw_improve_material_message_v1(72, &h(1), &c, Some(&bond(1))), "the tag");
        assert_ne!(m, palw_improve_material_message_v1(71, &h(2), &c, Some(&bond(1))), "the network");
        assert_ne!(m, palw_improve_material_message_v1(71, &h(1), &c, Some(&bond(2))), "the signer");
        assert_ne!(m, palw_improve_material_message_v1(71, &h(1), &c, None), "a bond or none");
        assert_ne!(m, palw_improve_material_message_v1(71, &h(1), &PalwHardCaseV1 { domain: 8, ..c }, Some(&bond(1))), "the payload");
    }
}
