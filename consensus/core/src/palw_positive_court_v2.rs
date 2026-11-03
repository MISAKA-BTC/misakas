//! Positive Court R2: isolated EXACT_STREAM prototype, with Panel-owned evidence.
//!
//! This is deliberately not a native consensus event or a source of Gate C credit.
//! Old claims, producer trace commitments and fraud verdict ownership are unchanged.
//! An invalid Panel computation rejects its candidate; it cannot convict the Miner.

use crate::palw_step_refute::{PalwDecodeTokenPinV1, PalwStepRefuteError};
use crate::{Hash64, palw_tir_court_v1::*, palw_tir_step_v1::*};
use borsh::{BorshDeserialize, BorshSerialize};

pub mod coordinator;
pub mod gate;
pub mod journal;
pub mod service;
pub use gate::AdmissionScopeV2;

pub const RESULT_CONTEXT: &[u8] = b"misaka/positive-court/result/v2";
pub const WITNESS_CONTEXT: &[u8] = b"misaka/positive-court/panel-witness/v2";

fn digest<T: BorshSerialize>(domain: &[u8], value: &T) -> Hash64 {
    Hash64::from_bytes(
        blake2b_simd::Params::new()
            .hash_length(64)
            .key(domain)
            .hash(&borsh::to_vec(value).expect("in-memory serialization"))
            .as_bytes()
            .try_into()
            .expect("64"),
    )
}

/// No trace root, intermediate values, or evidence construction obligation.
/// Network, input, tokenizer, program/class, generation seed and execution rules
/// are fixed by `job`; response consent is separately committed by `service_policy`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MinerResultV2 {
    pub version: u16,
    pub network: Hash64,
    pub branch: Hash64,
    pub ruleset: Hash64,
    pub claim: Hash64,
    pub job: crate::palw_v2::PalwJobContextV2,
    pub artifact: Hash64,
    pub output: Hash64,
    #[borsh(serialize_with = "serialize_sampling", deserialize_with = "deserialize_sampling")]
    pub sampling: crate::palw_decode_select_v2::PalwDecodeSamplingV2,
    pub service_policy: Hash64,
    pub dependencies: Hash64,
}
impl MinerResultV2 {
    pub fn id(&self) -> Hash64 {
        digest(b"misaka/positive-result/v2", self)
    }
}
fn serialize_sampling<W: std::io::Write>(x: &crate::palw_decode_select_v2::PalwDecodeSamplingV2, w: &mut W) -> std::io::Result<()> {
    (x.seed, x.temperature_q).serialize(w)
}
fn deserialize_sampling<R: std::io::Read>(r: &mut R) -> std::io::Result<crate::palw_decode_select_v2::PalwDecodeSamplingV2> {
    let (seed, temperature_q) = <([u8; 32], u32)>::deserialize_reader(r)?;
    Ok(crate::palw_decode_select_v2::PalwDecodeSamplingV2 { seed, temperature_q })
}

/// The signer is the Panel which built this execution, never implicitly Miner.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PanelWitnessV2 {
    pub version: u16,
    pub result: Hash64,
    pub policy: Hash64,
    pub attempt: u32,
    pub binding: PalwTirStepBindingV1,
    pub generated: Vec<u32>,
}
impl PanelWitnessV2 {
    pub fn id(&self) -> Hash64 {
        digest(b"misaka/positive-witness/v2", self)
    }
}

/// Identity/authentication only, not a computation certificate. Resolved keys
/// must come from the admitted Miner/Panel identity, not from an evidence payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureV2(pub Vec<u8>);
impl SignatureV2 {
    pub fn verifies(&self, key: &[u8], id: &Hash64, context: &[u8]) -> bool {
        crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(key, id.as_byte_slice(), &self.0, context)
    }
}

/// Explicit private process limits. These are not proposed mainnet parameters.
/// Total evaluation reservation is a conservative ceiling: every leaf reserves
/// a full court evaluation, including failed parts. No per-part reset of a claim cap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ExactPolicyV2 {
    pub max_leaves: u64,
    pub max_part_bytes: u64,
    pub max_part_heap: u64,
    pub max_total_bytes: u64,
    pub max_elements_per_leaf: u64,
    pub max_terms_per_leaf: u64,
    pub max_total_elements: u64,
    pub max_total_terms: u64,
    pub max_commit_bytes: u64,
    pub deadline: u64,
}
impl ExactPolicyV2 {
    pub fn id(&self) -> Hash64 {
        digest(b"misaka/positive-exact-policy/v2", self)
    }
    fn rules(&self, prompt_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1) -> PalwTirCourtRulesV1 {
        PalwTirCourtRulesV1 {
            max_step_leaf_count: self.max_leaves,
            prompt_form,
            limits: misaka_palw_tir::demand::DemandLimits {
                max_elements: self.max_elements_per_leaf,
                max_terms: self.max_terms_per_leaf,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaimStatusV2 {
    Checking,
    Passed,
    Refuted,
    EndedUnverified,
    Revoked,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WitnessStatusV2 {
    Checking,
    Complete,
    Rejected,
    Incomplete,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorV2 {
    Authentication,
    Binding,
    UnsupportedProfile,
    Budget,
    Order,
    PanelEvidence,
    Incomplete,
    Closed,
    Deadline,
    Wire,
}

/// One canonical leaf plus its trace bridge if and only if it is a logits leaf.
/// Every logits lane is checked, including losing vocabulary lanes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExactPartV2 {
    pub cone: PalwTirConeRefutationV1,
    pub trace: Option<PalwTirTraceEventDisclosureV1>,
}

/// Cannot be deserialized. A persisted `Passed` flag is never an authority:
/// restart must reauthenticate commitments and replay the bounded input parts.
/// This object owns no model weights. The ordinary TIR court authenticates the
/// supplied inventory openings against the registered artifact on each part.
pub struct ExactCaseV2 {
    result: MinerResultV2,
    witness: PanelWitnessV2,
    policy: ExactPolicyV2,
    rules: PalwTirCourtRulesV1,
    verified: PalwTirVerifiedBindingV1,
    next: u64,
    bytes: u64,
    row: u32,
    lane: u64,
    best: Option<(i128, u32)>,
    claim_status: ClaimStatusV2,
    witness_status: WitnessStatusV2,
}

impl ExactCaseV2 {
    /// Private admission only. `registered` and role keys are trusted admission
    /// inputs. A future native ingress must resolve them from its rooted state.
    #[allow(clippy::too_many_arguments)]
    pub fn admit(
        result: MinerResultV2,
        miner_signature: &SignatureV2,
        miner_key: &[u8],
        witness: PanelWitnessV2,
        panel_signature: &SignatureV2,
        panel_key: &[u8],
        registered: &crate::palw_tir_class_v1::PalwTirClassV1,
        policy: ExactPolicyV2,
        prompt_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
        now: u64,
        scope: AdmissionScopeV2,
    ) -> Result<Self, ErrorV2> {
        if now >= policy.deadline {
            return Err(ErrorV2::Deadline);
        }
        if policy.max_part_bytes == 0 || policy.max_part_heap == 0 || policy.max_total_bytes == 0 || policy.max_commit_bytes == 0 {
            return Err(ErrorV2::Budget);
        }
        if borsh::object_length(&result).map_err(|_| ErrorV2::Wire)? as u64 > policy.max_commit_bytes
            || borsh::object_length(&witness).map_err(|_| ErrorV2::Wire)? as u64 > policy.max_commit_bytes
        {
            return Err(ErrorV2::Budget);
        }
        if scope.check_result(&result).is_err()
            || result.version != 2
            || witness.version != 2
            || witness.result != result.id()
            || witness.policy != policy.id()
            || &witness.binding.class != registered
            || witness.binding.job_context != result.job
            || witness.binding.artifact_root != result.artifact
            || witness.generated.len() != result.job.exact_decode_tokens as usize
        {
            return Err(ErrorV2::Binding);
        }
        if !miner_signature.verifies(miner_key, &result.id(), RESULT_CONTEXT)
            || !panel_signature.verifies(panel_key, &witness.id(), WITNESS_CONTEXT)
        {
            return Err(ErrorV2::Authentication);
        }
        // This first version implements fixed-length greedy TIR, not opaque,
        // masked/V4 decoding or FULL_RELATION_DIRECT_V1 with unknown soundness.
        if !result.sampling.is_greedy() {
            return Err(ErrorV2::UnsupportedProfile);
        }
        let verified = verify_tir_binding_v1(&witness.binding, policy.max_leaves).map_err(|_| ErrorV2::Binding)?;
        // This isolated legacy prototype implements only the existing i32
        // logits profile. Wider traces are explicitly unsupported here; they
        // must not acquire r1 acceptance through unrelated work-in-progress APIs.
        let program = &verified.space.program;
        if program.blocks[program.schedule.post as usize].nodes[program.logits as usize].out.dtype != misaka_palw_tir::DType::I32 {
            return Err(ErrorV2::UnsupportedProfile);
        }
        let n = witness.binding.step_leaf_count;
        if n == 0
            || n.checked_mul(policy.max_elements_per_leaf).filter(|x| *x <= policy.max_total_elements).is_none()
            || n.checked_mul(policy.max_terms_per_leaf).filter(|x| *x <= policy.max_total_terms).is_none()
        {
            return Err(ErrorV2::Budget);
        }
        let vocab = verified.space.program.blocks[verified.space.program.schedule.post as usize].nodes
            [verified.space.program.logits as usize]
            .out
            .shape
            .iter()
            .try_fold(1u64, |a, d| match d {
                misaka_palw_tir::Dim::Fixed(x) => a.checked_mul(*x as u64),
                _ => None,
            })
            .ok_or(ErrorV2::UnsupportedProfile)?;
        if vocab == 0 || vocab > u32::MAX as u64 {
            return Err(ErrorV2::UnsupportedProfile);
        }
        if witness.generated.iter().any(|x| *x as u64 >= vocab) {
            return Err(ErrorV2::PanelEvidence);
        }
        Ok(Self {
            result,
            witness,
            policy,
            rules: policy.rules(prompt_form),
            verified,
            next: 0,
            bytes: 0,
            row: 0,
            lane: 0,
            best: None,
            claim_status: ClaimStatusV2::Checking,
            witness_status: WitnessStatusV2::Checking,
        })
    }

    pub fn statuses(&self) -> (ClaimStatusV2, WitnessStatusV2) {
        (self.claim_status, self.witness_status)
    }
    pub fn cursor(&self) -> u64 {
        self.next
    }
    pub fn charged_bytes(&self) -> u64 {
        self.bytes
    }
    pub fn result(&self) -> &MinerResultV2 {
        &self.result
    }
    pub fn witness(&self) -> &PanelWitnessV2 {
        &self.witness
    }

    /// Each inner object is structurally scanned before Borsh allocates vectors.
    /// A forged length prefix cannot allocate independently of the byte/heap cap.
    pub fn submit_encoded(&mut self, cone: &[u8], trace: Option<&[u8]>, now: u64) -> Result<(), ErrorV2> {
        self.ensure_open(now)?;
        let length = (cone.len() as u64).checked_add(trace.map_or(0, |x| x.len() as u64)).ok_or(ErrorV2::Budget)?;
        self.charge(length)?;
        let part = (|| {
            let heap = scan_heap::<PalwTirConeRefutationV1>(cone)?
                .checked_add(trace.map(scan_heap::<PalwTirTraceEventDisclosureV1>).transpose()?.unwrap_or(0))
                .ok_or(ErrorV2::Budget)?;
            if heap > self.policy.max_part_heap {
                return Err(ErrorV2::Budget);
            }
            Ok(ExactPartV2 {
                cone: borsh::from_slice(cone).map_err(|_| ErrorV2::Wire)?,
                trace: trace.map(|b| borsh::from_slice(b).map_err(|_| ErrorV2::Wire)).transpose()?,
            })
        })();
        match part.and_then(|p| self.check_part(&p)) {
            Ok(()) => {
                self.next += 1;
                Ok(())
            }
            Err(e) => {
                self.witness_status = WitnessStatusV2::Rejected;
                Err(e)
            }
        }
    }
    fn charge(&mut self, length: u64) -> Result<(), ErrorV2> {
        if length > self.policy.max_part_bytes {
            return Err(ErrorV2::Budget);
        }
        let total = self.bytes.checked_add(length).ok_or(ErrorV2::Budget)?;
        if total > self.policy.max_total_bytes {
            return Err(ErrorV2::Budget);
        }
        self.bytes = total;
        Ok(())
    }
    fn ensure_open(&mut self, now: u64) -> Result<(), ErrorV2> {
        if now >= self.policy.deadline {
            self.expire(now);
            return Err(ErrorV2::Deadline);
        }
        if self.claim_status != ClaimStatusV2::Checking || self.witness_status != WitnessStatusV2::Checking {
            return Err(ErrorV2::Closed);
        }
        Ok(())
    }
    fn check_part(&mut self, part: &ExactPartV2) -> Result<(), ErrorV2> {
        let c = &part.cone;
        if c.binding != self.witness.binding {
            return Err(ErrorV2::Binding);
        }
        if c.output_opening.leaf_index != self.next || self.next >= self.witness.binding.step_leaf_count {
            return Err(ErrorV2::Order);
        }
        if let Some(pin) = &c.decode_tokens {
            let ids = match pin {
                PalwDecodeTokenPinV1::TiledV1(p) => &p.generated_token_ids,
                PalwDecodeTokenPinV1::Base0V1(p) => &p.generated_token_ids,
                _ => return Err(ErrorV2::UnsupportedProfile),
            };
            if ids != &self.witness.generated {
                return Err(ErrorV2::PanelEvidence);
            }
        }
        // This verdict belongs to PANEL's committed trace. It is deliberately
        // never forwarded as a conviction of the Miner's result commitment.
        exact_ok(check_tir_cone_refutation_v1(c, &self.rules))?;
        let leaf = self.verified.space.leaf_at(&self.result.job, self.next).ok_or(ErrorV2::Order)?;
        let PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. } = leaf.kind else {
            return if part.trace.is_none() { Ok(()) } else { Err(ErrorV2::PanelEvidence) };
        };
        let p = &self.verified.space.program;
        if occurrence as usize != self.verified.space.occurrences().len() - 1 || node != p.logits {
            return if part.trace.is_none() { Ok(()) } else { Err(ErrorV2::PanelEvidence) };
        }
        let row = leaf.position.checked_add(1).and_then(|x| x.checked_sub(self.verified.job.prefill)).ok_or(ErrorV2::Order)?;
        if row != self.row || first_element != self.lane {
            return Err(ErrorV2::Order);
        }
        let trace = part.trace.as_ref().ok_or(ErrorV2::Incomplete)?;
        if trace.binding() != &self.witness.binding {
            return Err(ErrorV2::Binding);
        }
        let verdict = match trace {
            PalwTirTraceEventDisclosureV1::Tiled { generated_token_ids, row_root, row_opening, tile_lanes, tile_opening, .. } => {
                if generated_token_ids != &self.witness.generated {
                    return Err(ErrorV2::PanelEvidence);
                }
                check_tir_logits_consistency_v1(
                    &PalwTirLogitsConsistencyV1 {
                        binding: c.binding.clone(),
                        step_opening: c.output_opening.clone(),
                        step_preimage: c.output_preimage.clone(),
                        trace: PalwTirTraceLanesV1::Tiled {
                            generated_token_ids: generated_token_ids.clone(),
                            row_root: *row_root,
                            row_opening: row_opening.clone(),
                            tile_lanes: tile_lanes.clone(),
                            tile_opening: tile_opening.clone(),
                        },
                    },
                    &self.rules,
                )
            }
            _ => return Err(ErrorV2::UnsupportedProfile),
        };
        exact_ok(verdict)?;
        let width = 4;
        if c.output_preimage.values_le.len() != leaf.value_count as usize * width {
            return Err(ErrorV2::PanelEvidence);
        }
        // Compare every lane, in order; greedy ties select the first lane.
        for bytes in c.output_preimage.values_le.chunks_exact(width) {
            let value = i32::from_le_bytes(bytes.try_into().map_err(|_| ErrorV2::Wire)?) as i128;
            if self.best.is_none_or(|(best, _)| value > best) {
                self.best = Some((value, self.lane as u32));
            }
            self.lane += 1;
        }
        let vocab = p.blocks[p.schedule.post as usize].nodes[p.logits as usize]
            .out
            .shape
            .iter()
            .try_fold(1u64, |a, d| match d {
                misaka_palw_tir::Dim::Fixed(x) => a.checked_mul(*x as u64),
                _ => None,
            })
            .ok_or(ErrorV2::Binding)?;
        if self.lane > vocab {
            return Err(ErrorV2::Order);
        }
        if self.lane == vocab {
            if self.best.map(|x| x.1) != self.witness.generated.get(row as usize).copied() {
                return Err(ErrorV2::PanelEvidence);
            }
            self.row += 1;
            self.lane = 0;
            self.best = None;
        }
        Ok(())
    }

    pub fn finish(&mut self, now: u64) -> Result<ClaimStatusV2, ErrorV2> {
        self.ensure_open(now)?;
        if self.next != self.witness.binding.step_leaf_count || self.row != self.result.job.exact_decode_tokens || self.lane != 0 {
            return Err(ErrorV2::Incomplete);
        }
        self.witness_status = WitnessStatusV2::Complete;
        let checked = crate::palw_attempt_rules_v1::palw_attempt_output_root_v1(&self.result.job, &self.witness.generated);
        self.claim_status = if checked == self.result.output { ClaimStatusV2::Passed } else { ClaimStatusV2::Refuted };
        Ok(self.claim_status)
    }
    pub fn expire(&mut self, now: u64) {
        if now >= self.policy.deadline && self.claim_status == ClaimStatusV2::Checking {
            self.claim_status = ClaimStatusV2::EndedUnverified;
            if self.witness_status == WitnessStatusV2::Checking {
                self.witness_status = WitnessStatusV2::Incomplete;
            }
        }
    }
    pub fn revoke(&mut self) {
        self.claim_status = ClaimStatusV2::Revoked;
    }
    // No credit-ready method: funding, carrier availability, dependencies and
    // current Gate C cannot be represented by caller-supplied booleans here.
}

fn exact_ok<T>(result: Result<T, PalwStepRefuteError>) -> Result<(), ErrorV2> {
    match result {
        Err(PalwStepRefuteError::NoFaultFound) => Ok(()),
        _ => Err(ErrorV2::PanelEvidence),
    }
}

fn scan_heap<T: crate::palw_native_resource_v1::wire::NativeWireScanV1>(bytes: &[u8]) -> Result<u64, ErrorV2> {
    use crate::palw_native_resource_v1::wire::*;
    let mut reader = NativeWireReaderV1::new(std::io::Cursor::new(bytes), bytes.len() as u64);
    let shape = T::scan_native_wire_v1(&mut reader).map_err(|_| ErrorV2::Wire)?;
    if reader.remaining() != 0 {
        return Err(ErrorV2::Budget);
    }
    Ok(shape.heap)
}

#[cfg(test)]
mod tests;
