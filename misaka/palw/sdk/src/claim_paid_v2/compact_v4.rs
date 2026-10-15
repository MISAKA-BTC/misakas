//! Actual V4 prover/verifier over a pinned native round, never a V3 relabel.
use super::*;
use kaspa_consensus_core::palw_claim_compact_v4::PalwClaimCompactPolicyV4;
use misaka_palw_kernel::claim_compact_v1::native_v4::*;
fn refused(e: impl ToString) -> Refusal {
    Refusal::new("CLAIM_COMPACT_V4", e.to_string())
}
impl VerifiedPaidClaimSourcesV2 {
    fn compact_registration_v4(&self) -> Result<CompactClaimRegistrationV4, Refusal> {
        let plan = self
            .authority
            .class
            .interactive_plan
            .as_ref()
            .filter(|p| p.version == 4)
            .ok_or_else(|| refused("not a compact V4 class"))?;
        let registration = self.route.paid_compact_contract_v4(&plan.id()).map_err(refused)?.registration;
        registration.validate_for_class(&self.authority.class, &self.route.ledger().map_err(refused)?).map_err(refused)?;
        Ok(registration)
    }
    fn check_compact_v4(
        &self,
        material: &PaidClaimMaterialWireV2,
        check: &PaidClaimCheckAuthorityV2,
    ) -> Result<PaidClaimVerificationResultV2, Refusal> {
        let ledger = self.route.ledger().map_err(refused)?;
        let outsider = misaka_palw_kernel::ledger::OutsiderV1 {
            ledger: &ledger,
            claim: self.authority.enrollment.base_claim,
            material,
            artifact: material,
            salt: [0; 64],
        };
        match self.authority.class.target {
            PaidClaimTargetV2::Program | PaidClaimTargetV2::Pipeline => check.verify_program_or_pipeline(&outsider),
            PaidClaimTargetV2::Segmented => check.verify_segmented(&outsider, material, &material.tokens),
            PaidClaimTargetV2::TypedSpec => check.verify_typed(&misaka_palw_kernel::spec::outsider::SpecOutsiderV1 {
                ledger: &ledger,
                claim: self.authority.enrollment.base_claim,
                material,
                artifact: material,
                snapshots: material,
                salt: [0; 64],
            }),
        }
        .map_err(refused)
    }
    /// Future source must already be locked; no receipt can be issued by this
    /// producer-only session. The resulting root needs native producer signing.
    pub fn prove_compact_material_v4(
        &self,
        material: &PaidClaimMaterialWireV2,
        policy: &PalwClaimCompactPolicyV4,
    ) -> Result<NativeCompactProofV4, Refusal> {
        policy.validate().map_err(refused)?;
        let registration = self.compact_registration_v4()?;
        let session = std::rc::Rc::new(
            CompactVerificationSessionV4::prover(
                &registration,
                self.check.seed(),
                policy.maximum_stream_words,
                policy.maximum_proof_bytes,
            )
            .map_err(refused)?,
        );
        let check = self.check.bind_compact_opening_v4(material, &registration, session.clone(), None).map_err(refused)?;
        let result = self.check_compact_v4(material, &check)?;
        if !matches!(result.finding, misaka_palw_kernel::ledger::OutsiderFindingV1::Clean) {
            return Err(refused(format!("actual native check failed: {:?}", result.finding)));
        }
        session.finish().map_err(refused)
    }
    /// Authenticates producer proof-root from the served native state before
    /// accepting any compact relation. Unused proofs and missing instances fail.
    pub fn verify_compact_material_v4(
        &self,
        material: &PaidClaimMaterialWireV2,
        proof: &NativeCompactProofV4,
        policy: &PalwClaimCompactPolicyV4,
    ) -> Result<PaidClaimVerificationResultV2, Refusal> {
        policy.validate().map_err(refused)?;
        let registration = self.compact_registration_v4()?;
        let round = self.round.as_ref().ok_or_else(|| refused("compact verification needs a native proof round"))?;
        let binding = self
            .route
            .paid_compact_proof_binding_v4(round.enrollment, round.round)
            .map_err(refused)?
            .ok_or_else(|| refused("producer proof root has not been included"))?;
        if proof.root() != binding.proof_root {
            return Err(refused("compact proof is not the signed producer root"));
        }
        let session = std::rc::Rc::new(
            CompactVerificationSessionV4::verifier(
                &registration,
                self.check.seed(),
                policy.maximum_stream_words,
                policy.maximum_proof_bytes,
                proof,
            )
            .map_err(refused)?,
        );
        let check = self.check.bind_compact_opening_v4(material, &registration, session.clone(), Some(proof)).map_err(refused)?;
        let result = self.check_compact_v4(material, &check)?;
        session.finish().map_err(refused)?;
        Ok(result)
    }
    pub fn compact_proof_binding_object_v4(&self, proof: &NativeCompactProofV4) -> Result<KernelRouteObjectV1, Refusal> {
        let registration = self.compact_registration_v4()?;
        if proof.version != 4 || proof.plan != registration.plan.id() || proof.seed != self.check.seed() {
            return Err(refused("proof belongs to another native round/plan/seed"));
        }
        let round = self.round.as_ref().ok_or_else(|| refused("proof binding needs native round"))?;
        Ok(KernelRouteObjectV1::PaidClaimV2 {
            object: PaidClaimObjectV2::BindCompactProofV4 { enrollment: round.enrollment, round: round.round, proof: proof.clone() },
        })
    }
}
pub fn compact_registration_object_v4(
    route: &PalwKernelRouteStateV1,
    registration: CompactClaimRegistrationV4,
) -> Result<KernelRouteObjectV1, Refusal> {
    registration.validate(&route.ledger().map_err(refused)?).map_err(refused)?;
    Ok(KernelRouteObjectV1::PaidClaimV2 { object: PaidClaimObjectV2::RegisterCompactContractV4 { registration } })
}
/// Preserve the clock floors of the existing class builder, then select the
/// exact previously registered V4 plan. No unsigned mutation of an old class.
pub fn compact_class_object_v4(
    route: &PalwKernelRouteStateV1,
    policy: &kaspa_consensus_core::palw_claim_challenge_v2::PalwClaimChallengePolicyV2,
    registration: Digest,
    challenge: misaka_palw_challenge::PostCommitChallengePolicyV1,
    source: &kaspa_consensus_core::palw_beacon_source_opt_in_v1::PalwBeaconSourcePolicyV1,
) -> Result<KernelRouteObjectV1, Refusal> {
    let registration = route.paid_compact_contract_v4(&registration).map_err(refused)?.registration;
    let mut object = super::paid_claim_interactive_class_object_v3(
        route,
        policy,
        registration.registrant,
        registration.contract.base_class,
        challenge,
        source,
        registration.plan.rounds,
        registration.plan.maximum_material_bytes,
    )?;
    let KernelRouteObjectV1::PaidClaimV2 { object: PaidClaimObjectV2::RegisterClass { class } } = &mut object else { unreachable!() };
    if class.maximum_stream_blocks != registration.plan.maximum_stream_blocks {
        return Err(refused("registered stream capacity differs from class policy"));
    }
    class.interactive_plan = Some(registration.plan.clone());
    class.validate().map_err(refused)?;
    registration.validate_for_class(class, &route.ledger().map_err(refused)?).map_err(refused)?;
    Ok(object)
}

/// Allocation-free frame walk for the existing material wire. It preserves the
/// old codec while bounding nested maps, shape vectors and snapshot payloads
/// before Borsh creates any allocation. Mathematical/root authority is checked
/// separately by the actual native verifier.
pub fn decode_material_bounded_v4(bytes: &[u8], maximum_bytes: u64, maximum_heap: u64) -> Result<PaidClaimMaterialWireV2, String> {
    struct Cursor<'a> {
        bytes: &'a [u8],
        heap: u128,
        maximum: u128,
    }
    impl<'a> Cursor<'a> {
        fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
            if n > self.bytes.len() {
                return Err("truncated paid material".into());
            }
            let (a, b) = self.bytes.split_at(n);
            self.bytes = b;
            Ok(a)
        }
        fn charge(&mut self, n: u128) -> Result<(), String> {
            self.heap = self.heap.checked_add(n).ok_or("material heap overflow")?;
            if self.heap > self.maximum {
                return Err("paid material decoded workspace exceeds capacity".into());
            }
            Ok(())
        }
        fn count(&mut self, min_width: usize, entry_heap: u128) -> Result<usize, String> {
            let n = u32::from_le_bytes(self.take(4)?.try_into().unwrap()) as usize;
            if min_width != 0 && n > self.bytes.len() / min_width {
                return Err("paid material count exceeds exact frame".into());
            }
            self.charge((n as u128).checked_mul(entry_heap).ok_or("material vector capacity overflow")?)?;
            Ok(n)
        }
        fn tensor(&mut self) -> Result<(), String> {
            let dtype = misaka_palw_kernel::plan::dtype_of_tag(self.take(1)?[0]).ok_or("unknown material dtype")?;
            let rank = self.count(8, 8)?;
            if rank > 12 {
                return Err("material tensor rank exceeds capacity".into());
            }
            let mut elements = 1u128;
            for _ in 0..rank {
                let d = u64::from_le_bytes(self.take(8)?.try_into().unwrap());
                elements = elements.checked_mul(u128::from(d)).ok_or("material shape overflow")?;
            }
            let n = self.count(1, 1)?;
            if elements.checked_mul(dtype.width() as u128) != Some(n as u128) {
                return Err("material tensor shape/data mismatch".into());
            }
            self.take(n)?;
            self.charge(elements.checked_mul(16).ok_or("material tensor decoded heap overflow")?)
        }
    }
    if bytes.len() as u64 > maximum_bytes {
        return Err("paid material frame exceeds capacity".into());
    }
    let mut c = Cursor { bytes, heap: bytes.len() as u128, maximum: u128::from(maximum_heap) };
    for keybytes in [9usize, 7] {
        let n = c.count(keybytes + 9, 512)?;
        for _ in 0..n {
            c.take(keybytes)?;
            c.tensor()?;
        }
    }
    let n = c.count(4 + 1 + 9, 512)?;
    for _ in 0..n {
        c.take(4)?;
        match c.take(1)?[0] {
            0 => {}
            1 => {
                c.take(2)?;
            }
            _ => return Err("material layer option invalid".into()),
        }
        c.tensor()?;
    }
    let n = c.count(8, 512)?;
    for _ in 0..n {
        c.take(4)?;
        let occurrences = c.count(4, 24)?;
        for _ in 0..occurrences {
            let nodes = c.count(9, 32)?;
            for _ in 0..nodes {
                c.tensor()?;
            }
        }
    }
    let n = c.count(8, 512)?;
    for _ in 0..n {
        c.take(4)?;
        let siblings = c.count(64, 64)?;
        c.take(siblings.checked_mul(64).ok_or("sibling bytes overflow")?)?;
    }
    let n = c.count(4, 4)?;
    c.take(n.checked_mul(4).ok_or("token bytes overflow")?)?;
    let n = c.count(72 + 8, 512)?;
    for _ in 0..n {
        c.take(72)?;
        for _ in 0..2 {
            let words = c.count(4, 4)?;
            c.take(words.checked_mul(4).ok_or("snapshot bytes overflow")?)?;
        }
    }
    if !c.bytes.is_empty() {
        return Err("paid material trailing bytes".into());
    }
    borsh::from_slice(bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod wire_tests {
    use super::*;
    #[test]
    fn material_borsh_lengths_are_bounded_before_nested_allocation() {
        let material = PaidClaimMaterialWireV2::default();
        let bytes = borsh::to_vec(&material).unwrap();
        decode_material_bounded_v4(&bytes, 4096, 4096).unwrap();
        for prefix in 0..bytes.len() {
            assert!(decode_material_bounded_v4(&bytes[..prefix], 4096, 4096).is_err());
        }
        let mut bomb = bytes.clone();
        bomb[..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_material_bounded_v4(&bomb, 4096, 4096).is_err());
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(decode_material_bounded_v4(&trailing, 4096, 4096).is_err());
    }
}
