//! Rooted DATA for replaying a TFO3 at its actual original native prefix.
//! This is neither an admission permit nor a paid archive-retention contract.
//! The bounded native loader must independently verify acceptance, Full UTXO
//! validation and the actual delta prefix before replaying the original policy.
use crate::{
    Hash64,
    palw_native_resource_v1::{NativeWireReaderV1, NativeWireShapeV1, typed::native_struct, wire::wire_bad},
    palw_positive_contract_v2::{PublicOriginalAdmissionV1, PublicOriginalReservationV1},
    palw_state_v2::{PalwBlockContextV2, PalwChainStateV2},
    tx::{Transaction, UtxoEntry},
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_positive_contract::{
    ContractError,
    identity::Digest,
    public_one_honest::OriginalId,
    rows::{AcceptedOrigin, NativeHash64},
};
use std::io::{self, Read};

pub(crate) const PUBLIC_TFP1_HISTORY_TABLE_V1: u8 = 206;

/// Fixed DATA captured while the actual accepted UTXOs are still available.
/// No wire decoder or caller-supplied digest creates this native handoff.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidatedTfp1OriginalInputsV1 {
    carrier: Hash64,
    populated_inputs: Digest,
    input_count: u32,
    fee: u64,
}
native_struct!(ValidatedTfp1OriginalInputsV1; carrier, populated_inputs, input_count, fee);

impl ValidatedTfp1OriginalInputsV1 {
    /// The UTXO owner supplies its validated transaction. Other FP forms do
    /// not acquire new requirements. Malformed wrappers produce no handoff
    /// and remain subject to the existing funded ingress refusal.
    pub fn prepare_v1(tx: &crate::tx::ValidatedTransaction<'_>) -> Result<Option<Self>, ContractError> {
        let Ok(payload) = crate::palw_tir_funded_original_envelope_v1::actual_funded_payload_v1(tx.tx) else { return Ok(None) };
        if !crate::palw_tir_funded_original_envelope_v3::recognizes_v3(payload) {
            return Ok(None);
        }
        crate::palw_tir_funded_original_envelope_v3::PalwTirFundedOriginalEnvelopeV3::decode_v3(payload)
            .map_err(|_| ContractError::NonCanonical)?;
        let seed = Self {
            carrier: tx.tx.id(),
            populated_inputs: crate::palw_positive_assignment_case_material_v2::assigned_case_inputs_id_v2(tx.tx.id(), &tx.entries)?,
            input_count: u32::try_from(tx.entries.len()).map_err(|_| ContractError::Budget)?,
            fee: tx.calculated_fee,
        };
        let origin = AcceptedOrigin {
            accepting_block: NativeHash64([0; 64]),
            carrier_block: NativeHash64([0; 64]),
            carrier_transaction: NativeHash64(seed.carrier.as_bytes()),
            accepted_tx_ordinal: 0,
            event_ordinal: 0,
            accepted_daa: 0,
            accepted_blue_score: 0,
        };
        crate::palw_positive_assignment_case_material_v2::check_native_populated_inputs_v2(
            origin,
            seed.input_count,
            seed.populated_inputs,
            seed.fee,
            tx.tx,
            &tx.entries,
            tx.calculated_fee,
        )?;
        if seed.input_count == 0 {
            return Err(ContractError::Scope);
        }
        Ok(Some(seed))
    }
    pub fn carrier_v1(&self) -> Hash64 {
        self.carrier
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AcceptedTfp1OriginalInputsV1 {
    seed: ValidatedTfp1OriginalInputsV1,
    origin: AcceptedOrigin,
    parent: Hash64,
}
impl AcceptedTfp1OriginalInputsV1 {
    pub(crate) fn bind_v1(seed: ValidatedTfp1OriginalInputsV1, origin: AcceptedOrigin, parent: Hash64) -> Result<Self, ContractError> {
        if seed.carrier.as_bytes() != origin.carrier_transaction.0
            || origin.event_ordinal != 0
            || origin.accepted_daa == 0
            || origin.accepted_blue_score == 0
            || [origin.accepting_block.0, origin.carrier_block.0, parent.as_bytes()].contains(&[0; 64])
        {
            return Err(ContractError::Scope);
        }
        Ok(Self { seed, origin, parent })
    }
    pub(crate) fn origin_v1(&self) -> AcceptedOrigin {
        self.origin
    }
    pub(crate) fn parent_v1(&self) -> Hash64 {
        self.parent
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Tfp1OriginalHistoryBoundaryV1 {
    version: u16,
    pub claim: Hash64,
    pub source: Hash64,
    pub original: OriginalId,
    pub origin: AcceptedOrigin,
    pub selected_parent: NativeHash64,
    pub pre_event_root: Digest,
    pub before_delta_end: u64,
    populated_inputs: Digest,
    input_count: u32,
    pub fee: u64,
    admission_inputs: Digest,
}
native_struct!(Tfp1OriginalHistoryBoundaryV1; version, claim, source, original, origin, selected_parent, pre_event_root,
    before_delta_end, populated_inputs, input_count, fee, admission_inputs);

impl Tfp1OriginalHistoryBoundaryV1 {
    pub const WIRE_BYTES: usize = 2 + 10 * 64 + 3 * 4 + 4 * 8;

    /// DATA construction is crate-local; the paired native writer is the only
    /// path that roots it. The prefix token comes from the full ordered
    /// builder; the fixed UTXO handoff comes from that exact accepted carrier.
    pub(crate) fn from_actual_data_v1(
        claim: Hash64,
        source: Hash64,
        original: OriginalId,
        origin: AcceptedOrigin,
        prefix: &crate::palw_state_v2::Tfp1OriginalPrefixV1,
        accepted: AcceptedTfp1OriginalInputsV1,
        admission: &PublicOriginalAdmissionV1<'_>,
        inputs: &PublicOriginalReservationV1<'_>,
    ) -> Result<Self, ContractError> {
        if origin != inputs.origin
            || origin != accepted.origin
            || prefix.claim_v1() != claim
            || prefix.carrier_v1() != accepted.seed.carrier
            || prefix.parent_v1() != accepted.parent
        {
            return Err(ContractError::Scope);
        }
        require_prefix_point_v1(prefix, origin)?;
        let row = Self {
            version: 1,
            claim,
            source,
            original,
            origin,
            selected_parent: NativeHash64(accepted.parent.as_bytes()),
            pre_event_root: Digest(prefix.root_v1().as_bytes()),
            before_delta_end: u64::try_from(prefix.end_v1()).map_err(|_| ContractError::Budget)?,
            populated_inputs: accepted.seed.populated_inputs,
            input_count: accepted.seed.input_count,
            fee: accepted.seed.fee,
            admission_inputs: admission_inputs_id_v1(admission, inputs)?,
        };
        row.check_shape_v1(&claim.as_bytes())?;
        Ok(row)
    }

    pub fn check_populated_data(&self, tx: &Transaction, entries: &[UtxoEntry], fee: u64) -> Result<(), ContractError> {
        self.check_shape_v1(&self.claim.as_bytes())?;
        crate::palw_positive_assignment_case_material_v2::check_native_populated_inputs_v2(
            self.origin,
            self.input_count,
            self.populated_inputs,
            self.fee,
            tx,
            entries,
            fee,
        )
    }

    pub(crate) fn check_admission_inputs_v1(
        &self,
        admission: &PublicOriginalAdmissionV1<'_>,
        inputs: &PublicOriginalReservationV1<'_>,
    ) -> Result<(), ContractError> {
        if inputs.origin != self.origin || admission_inputs_id_v1(admission, inputs)? != self.admission_inputs {
            return Err(ContractError::Scope);
        }
        Ok(())
    }

    pub(crate) fn decode_row(key: &[u8], bytes: &[u8]) -> Result<Self, ContractError> {
        if bytes.len() != Self::WIRE_BYTES {
            return Err(ContractError::NonCanonical);
        }
        let row: Self = borsh::from_slice(bytes).map_err(|_| ContractError::NonCanonical)?;
        row.check_shape_v1(key)?;
        Ok(row)
    }

    fn check_shape_v1(&self, key: &[u8]) -> Result<(), ContractError> {
        if self.version != 1 || key != self.claim.as_byte_slice() {
            return Err(ContractError::Identity);
        }
        if [self.claim, self.source].contains(&Hash64::default())
            || [self.original.0, self.pre_event_root, self.populated_inputs, self.admission_inputs].contains(&Digest([0; 64]))
            || [self.origin.accepting_block.0, self.origin.carrier_block.0, self.origin.carrier_transaction.0, self.selected_parent.0]
                .contains(&[0; 64])
            || self.input_count == 0
            || self.origin.event_ordinal != 0
            || self.origin.accepted_daa == 0
            || self.origin.accepted_blue_score == 0
        {
            return Err(ContractError::Scope);
        }
        Ok(())
    }

    pub(crate) fn check_insert_at_v1(&self, at: &PalwBlockContextV2) -> Result<(), ContractError> {
        crate::palw_positive_contract_v2::require_current_public_origin_v1(self.origin, at)
    }

    pub(crate) fn check_state_at_v1(&self, state: &PalwChainStateV2, at: &PalwBlockContextV2) -> Result<(), ContractError> {
        self.check_shape_v1(&self.claim.as_bytes())?;
        crate::palw_positive_contract_v2::check_public_origin_point_v1(self.origin, at)?;
        let route = state.kernel_route_state_v1().ok_or(ContractError::Scope)?;
        let bytes = route
            .aux
            .get(&(crate::palw_positive_contract_v2::PUBLIC_TFP1_LINK_TABLE_V1, self.claim.as_bytes().to_vec()))
            .ok_or(ContractError::Scope)?;
        let link = crate::palw_positive_contract_v2::tfp1_original::native_link::Tfp1PublicOriginalLinkV1::decode_row(
            &self.claim.as_bytes(),
            bytes,
        )?;
        link.check_state_at_v1(state, at)?;
        if link.source_v1() != self.source || link.original_v1() != self.original || link.carrier_v1().origin != self.origin {
            return Err(ContractError::Scope);
        }
        Ok(())
    }

    pub(crate) fn scan_row_v1(key: &[u8], reader: &mut NativeWireReaderV1<impl Read>) -> io::Result<NativeWireShapeV1> {
        if key.len() != 64 || reader.remaining() != Self::WIRE_BYTES as u64 {
            return Err(wire_bad("invalid TFP1 history width"));
        }
        reader.step()?;
        let mut bytes = [0; Self::WIRE_BYTES];
        reader.read_exact(&mut bytes)?;
        Self::decode_row(key, &bytes).map_err(|_| wire_bad("invalid TFP1 history row"))?;
        Ok(NativeWireShapeV1::default())
    }
}

fn require_prefix_point_v1(prefix: &crate::palw_state_v2::Tfp1OriginalPrefixV1, origin: AcceptedOrigin) -> Result<(), ContractError> {
    crate::palw_positive_contract_v2::require_current_public_origin_v1(origin, &prefix.point_v1())
}

/// Native record lookup, never a caller-supplied archive boundary. A matching
/// row still needs actual historical transaction/UTXO/prefix reconstruction.
pub fn rooted_tfp1_original_history_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
) -> Result<Option<Tfp1OriginalHistoryBoundaryV1>, ContractError> {
    let Some(route) = state.kernel_route_state_v1() else { return Ok(None) };
    let key = claim.as_bytes();
    let Some(bytes) = route.aux.get(&(PUBLIC_TFP1_HISTORY_TABLE_V1, key.to_vec())) else { return Ok(None) };
    let row = Tfp1OriginalHistoryBoundaryV1::decode_row(&key, bytes)?;
    row.check_state_at_v1(state, state.last_point().ok_or(ContractError::Scope)?)?;
    Ok(Some(row))
}

/// Commit raw timing inputs, not just their derived deadlines. The original
/// policy/profile still has to reconstruct these preimages; a hash cannot.
fn admission_inputs_id_v1(
    admission: &PublicOriginalAdmissionV1<'_>,
    inputs: &PublicOriginalReservationV1<'_>,
) -> Result<Digest, ContractError> {
    let n = inputs.notice;
    let c = inputs.case;
    let notice = [
        n.draw_ready_by,
        n.material_ready_by,
        n.cold_acquisition_daa,
        n.full_inspection_daa,
        n.notice_inclusion_daa,
        n.missing_material_detected_by,
        n.material_request_inclusion_daa,
    ];
    let case = [c.material_delivery_daa, c.full_inspection_daa, c.localization_daa, c.filing_daa, c.adjudication_and_recovery_daa];
    let digest = crate::palw_court_shared_tir_stream_v1::stream_digest(
        b"misaka-palw/tfp1-original-history-inputs/v1",
        &(
            (
                admission.scope,
                admission.job,
                admission.class,
                admission.artifact,
                admission.miner,
                admission.prompt,
                admission.initial_state,
                admission.dependencies,
                admission.service_contract,
                admission.max_manifest_bytes,
                admission.manifest_scope_limits,
                admission.manifest_total_limit,
            ),
            (
                inputs.origin,
                inputs.service_obligation,
                notice,
                case,
                inputs.initial_supply,
                inputs.rescue,
                inputs.usage,
                inputs.budget,
            ),
        ),
    )
    .map_err(|_| ContractError::NonCanonical)?;
    Ok(Digest(digest.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::{ScriptPublicKey, TransactionInput, TransactionOutpoint, TransactionOutput};

    #[test]
    fn history_codec_and_populated_inputs_are_exact_data() {
        let h = Hash64::from_u64_word;
        let tx = Transaction::new(
            0,
            vec![TransactionInput::new(TransactionOutpoint::new(h(1), 0), vec![], 0, 0)],
            vec![TransactionOutput::new(80, ScriptPublicKey::default())],
            0,
            crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT,
            0,
            vec![],
        );
        let entries = [UtxoEntry::new(100, ScriptPublicKey::default(), 1, false)];
        let row = Tfp1OriginalHistoryBoundaryV1 {
            version: 1,
            claim: h(2),
            source: h(3),
            original: OriginalId(Digest(h(4).as_bytes())),
            origin: AcceptedOrigin {
                accepting_block: NativeHash64(h(5).as_bytes()),
                carrier_block: NativeHash64(h(6).as_bytes()),
                carrier_transaction: NativeHash64(tx.id().as_bytes()),
                accepted_tx_ordinal: 1,
                event_ordinal: 0,
                accepted_daa: 2,
                accepted_blue_score: 3,
            },
            selected_parent: NativeHash64(h(7).as_bytes()),
            pre_event_root: Digest(h(8).as_bytes()),
            before_delta_end: 0,
            populated_inputs: crate::palw_positive_assignment_case_material_v2::assigned_case_inputs_id_v2(tx.id(), &entries).unwrap(),
            input_count: 1,
            fee: 20,
            admission_inputs: Digest(h(9).as_bytes()),
        };
        let bytes = borsh::to_vec(&row).unwrap();
        assert_eq!(bytes.len(), Tfp1OriginalHistoryBoundaryV1::WIRE_BYTES);
        assert_eq!(Tfp1OriginalHistoryBoundaryV1::decode_row(&h(2).as_bytes(), &bytes), Ok(row));
        let scan = |key: &[u8], bytes: &[u8]| {
            let mut reader = NativeWireReaderV1::new(bytes, bytes.len() as u64);
            Tfp1OriginalHistoryBoundaryV1::scan_row_v1(key, &mut reader)
        };
        assert!(scan(&h(2).as_bytes(), &bytes).is_ok());
        assert!(scan(&h(3).as_bytes(), &bytes).is_err());
        assert!(scan(&h(2).as_bytes(), &bytes[1..]).is_err());
        assert!(scan(&h(2).as_bytes(), &[bytes.as_slice(), &[0]].concat()).is_err());
        for wrong in [
            Tfp1OriginalHistoryBoundaryV1 { version: 2, ..row },
            Tfp1OriginalHistoryBoundaryV1 { input_count: 0, ..row },
            Tfp1OriginalHistoryBoundaryV1 { admission_inputs: Digest([0; 64]), ..row },
            Tfp1OriginalHistoryBoundaryV1 { origin: AcceptedOrigin { event_ordinal: 1, ..row.origin }, ..row },
        ] {
            assert!(scan(&h(2).as_bytes(), &borsh::to_vec(&wrong).unwrap()).is_err());
        }
        assert_eq!(row.check_populated_data(&tx, &entries, 20), Ok(()));
        assert!(row.check_populated_data(&tx, &entries, 19).is_err());
        let altered = [UtxoEntry::new(100, ScriptPublicKey::default(), 2, false)];
        assert!(row.check_populated_data(&tx, &altered, 20).is_err());
        let mut other = tx.clone();
        other.outputs[0].value += 1;
        assert!(row.check_populated_data(&other, &entries, 19).is_err());
        other.finalize();
        assert!(row.check_populated_data(&other, &entries, 19).is_err());
    }
}
