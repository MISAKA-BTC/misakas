//! Identity-only signed carriage for the ORIGINAL full-claim TIR route.
//! No lifecycle tag, activation, payment or computation-correctness authority is
//! assigned here. Native ingress must authenticate the bond, frozen allocation,
//! literal custody, clock, resource permits and ordinary receipt independently.
use crate::palw_backend::PalwTirPublicPreparedDescriptorV1;
use crate::palw_panel_v2::PalwSeatReceiptV2;
use crate::palw_tir_public_custody_v1::PalwTirPublicCustodyGrantV1;
use crate::palw_tir_public_owned_v1::{
    PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1, PalwTirPublicOwnedActionV1 as OwnedAction, PalwTirPublicOwnedV1, PalwTirPublicPreparationV1,
    PalwTirPublicWorkPhaseV1,
};
use crate::{Hash64, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};

pub const PALW_TIR_PUBLIC_CONTROL_CONTEXT_V1: &[u8] = b"misaka-palw/tir-public-control/v1/mldsa87";
pub const PALW_TIR_PUBLIC_GRANT_READY_CONTEXT_V1: &[u8] = b"misaka-palw/tir-grant-ready/v1/mldsa87";
pub const PALW_TIR_PUBLIC_CONTROL_MAX_BYTES_V1: u64 = 90_000;

fn bounded<T: BorshDeserialize>(bytes: &[u8], native_maximum: u64) -> Result<T, String> {
    if bytes.len() as u64 > native_maximum.min(PALW_TIR_PUBLIC_CONTROL_MAX_BYTES_V1) {
        return Err("TIR control exceeds its native carrier bound before decoding".into());
    }
    borsh::from_slice(bytes).map_err(|e| e.to_string())
}
fn message<T: BorshSerialize>(domain: &[u8], network: Hash64, fields: &T) -> Result<Hash64, String> {
    let bytes = borsh::to_vec(&(network, fields)).map_err(|e| e.to_string())?;
    Ok(Hash64::from_bytes(blake2b_simd::Params::new().hash_length(64).key(domain).hash(&bytes).as_bytes().try_into().expect("64")))
}
fn signature_shape(signature: &[u8]) -> Result<(), String> {
    if signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {
        return Err("TIR control signature has a noncanonical length".into());
    }
    Ok(())
}
fn verify<F>(key: &[u8], signature: &[u8], digest: Hash64, context: &[u8], check: F) -> Result<(), String>
where
    F: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    signature_shape(signature)?;
    if key.len() != crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN || !check(key, digest.as_byte_slice(), signature, context) {
        return Err("TIR control is not signed by its literal native bond".into());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwTirPublicControlTargetV1 {
    /// No proposed future clock: native Open assigns its own accepting DAA.
    OwnerOpen {
        owner: PalwBondKeyV2,
    },
    Acquisition {
        owner: PalwBondKeyV2,
        reserved_daa: u64,
    },
    /// Claim-wide, never an owner's reservation or a synthetic owner row.
    Preparation {
        original_accepted_daa: u64,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwTirPublicControlActionV1 {
    Open,
    Acquisition(OwnedAction),
    Preparation(OwnedAction),
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwTirPublicControlV1 {
    pub version: u16,
    pub claim: Hash64,
    pub allocation_id: Hash64,
    pub source_id: Hash64,
    pub target: PalwTirPublicControlTargetV1,
    pub expected_turn: u64,
    pub signer: PalwBondKeyV2,
    pub action: PalwTirPublicControlActionV1,
    pub signature: Vec<u8>,
}
impl BorshDeserialize for PalwTirPublicControlV1 {
    fn deserialize_reader<R: std::io::Read>(r: &mut R) -> std::io::Result<Self> {
        Ok(Self {
            version: BorshDeserialize::deserialize_reader(r)?,
            claim: BorshDeserialize::deserialize_reader(r)?,
            allocation_id: BorshDeserialize::deserialize_reader(r)?,
            source_id: BorshDeserialize::deserialize_reader(r)?,
            target: BorshDeserialize::deserialize_reader(r)?,
            expected_turn: BorshDeserialize::deserialize_reader(r)?,
            signer: BorshDeserialize::deserialize_reader(r)?,
            action: BorshDeserialize::deserialize_reader(r)?,
            signature: crate::palw_public_binding_v1::read_signature_v1(r)?,
        })
    }
}
impl PalwTirPublicControlV1 {
    fn check_payload_v1(&self) -> Result<(), String> {
        use PalwTirPublicControlActionV1 as A;
        use PalwTirPublicControlTargetV1 as T;
        if self.version != 1
            || self.claim == Hash64::default()
            || self.allocation_id == Hash64::default()
            || self.source_id == Hash64::default()
            || borsh::object_length(self).map_err(|e| e.to_string())? as u64 > PALW_TIR_PUBLIC_CONTROL_MAX_BYTES_V1
        {
            return Err("TIR control has no bounded original claim/allocation/source identity".into());
        }
        match (&self.target, &self.action) {
            (T::OwnerOpen { owner }, A::Open) if *owner == self.signer && self.expected_turn == 0 => {}
            (T::Acquisition { owner, reserved_daa }, A::Acquisition(action)) => {
                match action {
                    OwnedAction::Demand { .. } | OwnedAction::Reacquire { .. } | OwnedAction::Collect { .. }
                        if self.signer == *owner => {}
                    OwnedAction::Begin { answerer, .. }
                    | OwnedAction::Part { answerer, .. }
                    | OwnedAction::Finish { answerer, .. }
                        if self.signer == *answerer => {}
                    OwnedAction::WorkDebit(debit)
                        if debit.scope.actor == self.signer
                            && debit.scope.owner == *owner
                            && debit.scope.reserved_daa == *reserved_daa
                            && debit.scope.turn == self.expected_turn
                            && debit.scope.phase != PalwTirPublicWorkPhaseV1::Capture => {}
                    _ => return Err("TIR acquisition action has another lane or literal signer".into()),
                }
                self.check_action_bounds_v1(action)?;
            }
            (T::Preparation { original_accepted_daa }, A::Preparation(action)) if self.expected_turn == 0 => {
                match action {
                    OwnedAction::WorkDebit(debit)
                        if debit.scope.actor == self.signer
                            && debit.scope.turn == 0
                            && debit.scope.reserved_daa == *original_accepted_daa
                            && debit.scope.phase == PalwTirPublicWorkPhaseV1::Capture
                            && debit.history.is_none() => {}
                    OwnedAction::PreparationReady { descriptor } if descriptor.source_id == self.source_id => {}
                    OwnedAction::PreparationCollect { actor } if *actor != self.signer => {}
                    _ => return Err("TIR preparation action substitutes an owner turn or another literal signer".into()),
                }
                self.check_action_bounds_v1(action)?;
            }
            _ => return Err("TIR control target and action are different native lanes".into()),
        }
        Ok(())
    }
    fn check_action_bounds_v1(&self, action: &OwnedAction) -> Result<(), String> {
        match action {
            OwnedAction::Part { bytes, .. } if bytes.is_empty() || bytes.len() > PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 => {
                return Err("TIR control part exceeds its fixed bounded carriage".into());
            }
            OwnedAction::Reacquire { proof, .. } if proof.siblings.len() > 64 => {
                return Err("TIR control history path exceeds its original u64 tree".into());
            }
            OwnedAction::WorkDebit(debit) => {
                let scope = &debit.scope;
                if scope.claim != self.claim
                    || scope.bundle_id != self.allocation_id
                    || scope.source_id != self.source_id
                    || debit.work.attempts == 0
                    || debit.history.as_ref().is_some_and(|p| p.siblings.len() > 64)
                {
                    return Err("TIR debit changes its signed original control scope or bounded history".into());
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub fn check_v1(&self) -> Result<(), String> {
        self.check_payload_v1()?;
        signature_shape(&self.signature)
    }
    pub fn decode_bounded_v1(bytes: &[u8], native_maximum: u64) -> Result<Self, String> {
        let value: Self = bounded(bytes, native_maximum)?;
        value.check_v1()?;
        Ok(value)
    }
    /// Unsigned construction may request this message with an empty signature.
    /// The supplied key must be resolved from `signer` by native ingress.
    pub fn signing_message_v1(&self, network: Hash64) -> Result<Hash64, String> {
        self.check_payload_v1()?;
        message(
            b"misaka-palw/tir-public-control/message/v1",
            network,
            &(
                self.version,
                self.claim,
                self.allocation_id,
                self.source_id,
                self.target,
                self.expected_turn,
                self.signer,
                &self.action,
            ),
        )
    }
    pub fn validate_signature_v1<F>(&self, network: Hash64, key: &[u8], check: F) -> Result<(), String>
    where
        F: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        self.check_v1()?;
        verify(key, &self.signature, self.signing_message_v1(network)?, PALW_TIR_PUBLIC_CONTROL_CONTEXT_V1, check)
    }
    /// Bond-key identity only. Active/floor/custody eligibility is action-specific
    /// and must still be checked by the native accepting transition.
    pub fn validate_native_signature_v1(&self, network: Hash64, state: &crate::palw_state_v2::PalwChainStateV2) -> Result<(), String> {
        let signer = state.bond(&self.signer).ok_or("TIR control literal signer bond is absent")?;
        self.validate_signature_v1(network, &signer.pubkey, crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1)
    }
    /// Pure exact-snapshot binding. Membership, clocks, paid resources and every
    /// inner action's native transition remain separate ingress requirements.
    pub fn check_owned_scope_v1(&self, owned: &PalwTirPublicOwnedV1) -> Result<(), String> {
        self.check_v1()?;
        let c = owned.contract_v1();
        let PalwTirPublicControlActionV1::Acquisition(action) = &self.action else {
            return Err("TIR control is not an owner continuation".into());
        };
        let expected = match action {
            OwnedAction::Demand { .. } | OwnedAction::Reacquire { .. } => owned.spent_v1().turns,
            _ => owned.active_v1().map_or(owned.spent_v1().turns, |p| p.turn),
        };
        if (self.claim, self.allocation_id, self.source_id) != (c.claim, c.bundle_id, c.source_id)
            || self.target != (PalwTirPublicControlTargetV1::Acquisition { owner: c.owner, reserved_daa: c.reserved_daa })
            || self.expected_turn != expected
        {
            return Err("TIR control substitutes the original owner, reservation, allocation, source or accepted turn".into());
        }
        if let OwnedAction::Begin { demand_id, .. }
        | OwnedAction::Part { demand_id, .. }
        | OwnedAction::Finish { demand_id, .. }
        | OwnedAction::Collect { demand_id } = action
        {
            if owned.active_v1().is_none_or(|p| p.demand_id != *demand_id) {
                return Err("TIR answer/collection names another accepted original demand".into());
            }
        }
        Ok(())
    }
    pub fn check_preparation_scope_v1(&self, preparation: &PalwTirPublicPreparationV1) -> Result<(), String> {
        self.check_v1()?;
        let c = preparation.contract_v1();
        if !matches!(self.action, PalwTirPublicControlActionV1::Preparation(_))
            || (self.claim, self.allocation_id, self.source_id) != (c.claim, c.allocation_id, c.source_id)
            || self.target != (PalwTirPublicControlTargetV1::Preparation { original_accepted_daa: c.original_accepted_daa })
        {
            return Err("TIR preparation changes the original claim-wide accepted scope".into());
        }
        if let PalwTirPublicControlActionV1::Preparation(OwnedAction::WorkDebit(debit)) = &self.action {
            if debit.scope.owner != c.producer {
                return Err("TIR preparation debit changes its original producer scope".into());
            }
        }
        Ok(())
    }
}

/// Atomic receipt credit + NEW raw-source custody grant + prepared Ready.
/// The outer signature additionally binds the accepted allocation (which the
/// pre-accept producer pledge cannot know). Native must validate the ordinary
/// receipt in the same transaction and roll ALL effects back if any check fails.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwTirPublicGrantReadyV1 {
    pub version: u16,
    pub claim: Hash64,
    pub allocation_id: Hash64,
    pub source_id: Hash64,
    pub signer: PalwBondKeyV2,
    pub receipt: PalwSeatReceiptV2,
    pub grant: PalwTirPublicCustodyGrantV1,
    pub descriptor: PalwTirPublicPreparedDescriptorV1,
    pub signature: Vec<u8>,
}
/// Lifecycle naming; the payload always includes the mandatory atomic Ready.
pub type PalwTirPublicGrantCarriageV1 = PalwTirPublicGrantReadyV1;
impl BorshDeserialize for PalwTirPublicGrantReadyV1 {
    fn deserialize_reader<R: std::io::Read>(r: &mut R) -> std::io::Result<Self> {
        Ok(Self {
            version: BorshDeserialize::deserialize_reader(r)?,
            claim: BorshDeserialize::deserialize_reader(r)?,
            allocation_id: BorshDeserialize::deserialize_reader(r)?,
            source_id: BorshDeserialize::deserialize_reader(r)?,
            signer: BorshDeserialize::deserialize_reader(r)?,
            receipt: PalwSeatReceiptV2 {
                claim: BorshDeserialize::deserialize_reader(r)?,
                verdict: BorshDeserialize::deserialize_reader(r)?,
                seat_bond: BorshDeserialize::deserialize_reader(r)?,
                signed_daa: BorshDeserialize::deserialize_reader(r)?,
                signature: crate::palw_public_binding_v1::read_signature_v1(r)?,
            },
            grant: BorshDeserialize::deserialize_reader(r)?,
            descriptor: BorshDeserialize::deserialize_reader(r)?,
            signature: crate::palw_public_binding_v1::read_signature_v1(r)?,
        })
    }
}
impl PalwTirPublicGrantReadyV1 {
    fn check_payload_v1(&self) -> Result<(), String> {
        if self.version != 1
            || self.claim == Hash64::default()
            || self.allocation_id == Hash64::default()
            || self.source_id == Hash64::default()
            || self.receipt.claim != self.claim
            || self.grant.claim != self.claim
            || self.receipt.seat_bond != self.signer
            || self.grant.seat != self.signer
            || self.descriptor.version != 1
            || self.descriptor.source_id != self.source_id
            || self.descriptor.manifest_digest == Hash64::default()
            || self.descriptor.retained_bytes == 0
            || borsh::object_length(self).map_err(|e| e.to_string())? as u64 > PALW_TIR_PUBLIC_CONTROL_MAX_BYTES_V1
        {
            return Err("TIR grant Ready changes literal seat, original source/allocation or bounded descriptor".into());
        }
        self.grant.check_receipt_v1(&self.receipt)
    }
    pub fn check_v1(&self) -> Result<(), String> {
        self.check_payload_v1()?;
        signature_shape(&self.signature)
    }
    pub fn decode_bounded_v1(bytes: &[u8], native_maximum: u64) -> Result<Self, String> {
        let value: Self = bounded(bytes, native_maximum)?;
        value.check_v1()?;
        Ok(value)
    }
    pub fn signing_message_v1(&self, network: Hash64) -> Result<Hash64, String> {
        self.check_payload_v1()?;
        message(
            b"misaka-palw/tir-grant-ready/message/v1",
            network,
            &(self.version, self.claim, self.allocation_id, self.source_id, self.signer, &self.receipt, &self.grant, self.descriptor),
        )
    }
    /// Does NOT replace ordinary receipt verification, raw grant verification,
    /// full-plan payment or native prepared-descriptor/source/clock checks.
    pub fn validate_signature_v1<F>(&self, network: Hash64, key: &[u8], check: F) -> Result<(), String>
    where
        F: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        self.check_v1()?;
        verify(key, &self.signature, self.signing_message_v1(network)?, PALW_TIR_PUBLIC_GRANT_READY_CONTEXT_V1, check)
    }
    pub fn validate_native_signature_v1(&self, network: Hash64, state: &crate::palw_state_v2::PalwChainStateV2) -> Result<(), String> {
        let signer = state.bond(&self.signer).ok_or("TIR grant Ready literal signer bond is absent")?;
        self.validate_signature_v1(network, &signer.pubkey, crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1)
    }
}

#[cfg(test)]
pub(crate) fn palw_tir_public_control_fixture_v1() -> PalwTirPublicControlV1 {
    tests::control()
}
#[cfg(test)]
pub(crate) fn palw_tir_public_grant_fixture_v1() -> PalwTirPublicGrantCarriageV1 {
    tests::grant()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_tir_public_source_v1::PalwTirPublicRequestV1 as Request;
    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(n), 0))
    }
    fn key() -> Vec<u8> {
        vec![1; crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN]
    }
    fn sig() -> Vec<u8> {
        vec![2; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN]
    }
    // Injected signature checks isolate identity/domain/codec contracts; these
    // fixtures do not claim MLDSA execution or native admission coverage.
    fn signed(digest: Hash64) -> Vec<u8> {
        let mut signature = sig();
        signature[..64].copy_from_slice(digest.as_byte_slice());
        signature
    }
    pub(super) fn control() -> PalwTirPublicControlV1 {
        PalwTirPublicControlV1 {
            version: 1,
            claim: h(1),
            allocation_id: h(2),
            source_id: h(3),
            target: PalwTirPublicControlTargetV1::Acquisition { owner: bond(7), reserved_daa: 10 },
            expected_turn: 4,
            signer: bond(8),
            action: PalwTirPublicControlActionV1::Acquisition(OwnedAction::Part {
                demand_id: h(4),
                answerer: bond(8),
                offset: 16_384,
                bytes: vec![1, 2, 3],
            }),
            signature: sig(),
        }
    }
    fn descriptor() -> PalwTirPublicPreparedDescriptorV1 {
        PalwTirPublicPreparedDescriptorV1 {
            version: 1,
            source_id: h(3),
            class_id: h(10),
            artifact_root: h(11),
            execution_root: h(12),
            trace_root: h(13),
            step_root: h(14),
            step_leaf_count: 19,
            decode_count: 3,
            vocab: 8,
            material_until_daa: 1000,
            manifest_digest: h(15),
            retained_bytes: 1024,
        }
    }
    pub(super) fn grant() -> PalwTirPublicGrantReadyV1 {
        let receipt = PalwSeatReceiptV2 {
            claim: h(1),
            verdict: crate::palw_panel_v2::PalwReceiptVerdictV2::Valid,
            seat_bond: bond(8),
            signed_daa: 20,
            signature: sig(),
        };
        let grant = PalwTirPublicCustodyGrantV1 {
            version: 1,
            claim: receipt.claim,
            seat: receipt.seat_bond,
            signed_daa: receipt.signed_daa,
            pledge_id: h(20),
            valid_receipt_digest: PalwTirPublicCustodyGrantV1::receipt_digest_v1(&receipt),
            signature: sig(),
        };
        PalwTirPublicGrantReadyV1 {
            version: 1,
            claim: h(1),
            allocation_id: h(2),
            source_id: h(3),
            signer: bond(8),
            receipt,
            grant,
            descriptor: descriptor(),
            signature: sig(),
        }
    }
    #[test]
    fn control_signature_binds_original_scope_turn_target_payload_and_network() {
        let mut original = control();
        let digest = original.signing_message_v1(h(99)).unwrap();
        original.signature = signed(digest);
        assert!(
            original
                .validate_signature_v1(h(99), &key(), |_, message, signature, context| {
                    signature[..64] == *message && context == PALW_TIR_PUBLIC_CONTROL_CONTEXT_V1
                })
                .is_ok()
        );
        for field in 0..7 {
            let mut changed = original.clone();
            match field {
                0 => changed.claim = h(90),
                1 => changed.allocation_id = h(90),
                2 => changed.source_id = h(90),
                3 => changed.expected_turn += 1,
                4 => changed.target = PalwTirPublicControlTargetV1::Acquisition { owner: bond(9), reserved_daa: 10 },
                5 => changed.target = PalwTirPublicControlTargetV1::Acquisition { owner: bond(7), reserved_daa: 11 },
                _ => {
                    let PalwTirPublicControlActionV1::Acquisition(OwnedAction::Part { bytes, .. }) = &mut changed.action else {
                        unreachable!()
                    };
                    bytes[0] ^= 1;
                }
            }
            assert_ne!(changed.signing_message_v1(h(99)).unwrap(), digest);
            assert!(changed.validate_signature_v1(h(99), &key(), |_, m, s, _| s[..64] == *m).is_err());
        }
        assert_ne!(original.signing_message_v1(h(100)).unwrap(), digest);
        assert!(
            original
                .validate_signature_v1(h(99), &key(), |_, _, _, c| c == crate::palw_history_custody_v1::PALW_HISTORY_GRANT_CONTEXT_V1)
                .is_err()
        );
        let mut foreign = original;
        foreign.signer = bond(9);
        assert!(foreign.check_v1().is_err(), "a different bond cannot carry this literal answerer");
    }
    #[test]
    fn explicit_preparation_open_and_owner_targets_cannot_be_interchanged() {
        let mut c = control();
        c.target = PalwTirPublicControlTargetV1::OwnerOpen { owner: c.signer };
        c.action = PalwTirPublicControlActionV1::Open;
        assert!(c.check_v1().is_err(), "Open cannot borrow an existing turn");
        c.expected_turn = 0;
        c.check_v1().unwrap();
        c.target = PalwTirPublicControlTargetV1::Preparation { original_accepted_daa: 10 };
        assert!(c.check_v1().is_err());
        c.action = PalwTirPublicControlActionV1::Preparation(OwnedAction::PreparationReady { descriptor: descriptor() });
        c.check_v1().unwrap();
        c.action = PalwTirPublicControlActionV1::Acquisition(OwnedAction::Demand { request: Request::StepLeaf { index: 0 } });
        assert!(c.check_v1().is_err(), "turn zero is not a substitute for the correct claim-wide lane");
        let (_, preparation, _) = crate::palw_tir_public_owned_v1::palw_tir_public_preparation_fixture_v1();
        let p = preparation.contract_v1();
        c.claim = p.claim;
        c.allocation_id = p.allocation_id;
        c.source_id = p.source_id;
        c.target = PalwTirPublicControlTargetV1::Preparation { original_accepted_daa: p.original_accepted_daa };
        c.action = PalwTirPublicControlActionV1::Preparation(OwnedAction::PreparationCollect { actor: p.producer });
        c.check_preparation_scope_v1(&preparation).unwrap();
        c.target = PalwTirPublicControlTargetV1::Preparation { original_accepted_daa: p.original_accepted_daa + 1 };
        assert!(c.check_preparation_scope_v1(&preparation).is_err());
    }
    #[test]
    fn grant_ready_commits_all_three_signed_records_and_original_descriptor() {
        let mut g = grant();
        let digest = g.signing_message_v1(h(99)).unwrap();
        g.signature = signed(digest);
        g.validate_signature_v1(h(99), &key(), |_, m, s, c| s[..64] == *m && c == PALW_TIR_PUBLIC_GRANT_READY_CONTEXT_V1).unwrap();
        for field in 0..5 {
            let mut changed = g.clone();
            match field {
                0 => changed.allocation_id = h(91),
                1 => changed.descriptor.manifest_digest = h(91),
                2 => changed.descriptor.material_until_daa += 1,
                3 => changed.grant.signature[0] ^= 1,
                _ => {
                    changed.receipt.signature[0] ^= 1;
                    changed.grant.valid_receipt_digest = PalwTirPublicCustodyGrantV1::receipt_digest_v1(&changed.receipt);
                }
            }
            assert_ne!(changed.signing_message_v1(h(99)).unwrap(), digest);
        }
        let mut unrelated = g.clone();
        unrelated.receipt.signature[0] ^= 1;
        assert!(unrelated.check_v1().is_err(), "the exact signed ordinary Valid is inside the grant");
        let mut sampled = g.clone();
        sampled.receipt.verdict = crate::palw_panel_v2::PalwReceiptVerdictV2::Sampled;
        sampled.grant.valid_receipt_digest = PalwTirPublicCustodyGrantV1::receipt_digest_v1(&sampled.receipt);
        assert!(sampled.check_v1().is_err());
        assert!(g.validate_signature_v1(h(99), &key(), |_, _, _, c| c == PALW_TIR_PUBLIC_CONTROL_CONTEXT_V1).is_err());
    }
    #[test]
    fn decode_bounds_carried_parts_receipt_signatures_and_outer_signatures_before_allocation() {
        let c = control();
        let encoded = borsh::to_vec(&c).unwrap();
        assert_eq!(PalwTirPublicControlV1::decode_bounded_v1(&encoded, encoded.len() as u64).unwrap(), c);
        assert!(PalwTirPublicControlV1::decode_bounded_v1(&encoded, encoded.len() as u64 - 1).is_err());
        let mut bomb =
            borsh::to_vec(&(c.version, c.claim, c.allocation_id, c.source_id, c.target, c.expected_turn, c.signer)).unwrap();
        bomb.extend([1, 3]); // Acquisition action, Part inner action.
        bomb.extend(borsh::to_vec(&(h(4), c.signer, 0u64)).unwrap());
        bomb.extend(u32::MAX.to_le_bytes());
        assert!(PalwTirPublicControlV1::decode_bounded_v1(&bomb, PALW_TIR_PUBLIC_CONTROL_MAX_BYTES_V1).is_err());
        let mut bomb = encoded[..encoded.len() - c.signature.len() - 4].to_vec();
        bomb.extend(u32::MAX.to_le_bytes());
        assert!(PalwTirPublicControlV1::decode_bounded_v1(&bomb, PALW_TIR_PUBLIC_CONTROL_MAX_BYTES_V1).is_err());
        let g = grant();
        let encoded = borsh::to_vec(&g).unwrap();
        assert_eq!(PalwTirPublicGrantReadyV1::decode_bounded_v1(&encoded, PALW_TIR_PUBLIC_CONTROL_MAX_BYTES_V1).unwrap(), g);
        let mut bomb = borsh::to_vec(&(
            g.version,
            g.claim,
            g.allocation_id,
            g.source_id,
            g.signer,
            g.receipt.claim,
            g.receipt.verdict,
            g.receipt.seat_bond,
            g.receipt.signed_daa,
        ))
        .unwrap();
        bomb.extend(u32::MAX.to_le_bytes());
        assert!(PalwTirPublicGrantReadyV1::decode_bounded_v1(&bomb, PALW_TIR_PUBLIC_CONTROL_MAX_BYTES_V1).is_err());
        let mut trailing = encoded;
        trailing.push(0);
        assert!(PalwTirPublicGrantReadyV1::decode_bounded_v1(&trailing, PALW_TIR_PUBLIC_CONTROL_MAX_BYTES_V1).is_err());
    }
}
