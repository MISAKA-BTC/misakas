//! Service projection of the signed pipelined material codec. The coordinator
//! binds an accepted demand, native target bound and literal responder first.
use super::{
    MATERIAL_BATCH_VERSION, MATERIAL_PIPELINED_MAX_BYTES, MaterialActionV1 as Action, MaterialTargetV1, SignedMaterialIntakeV1,
    material_batch_draft_id_v1,
};
use crate::Hash64;
use crate::palw_court_service_v1::{ServiceCost, ServicePacket, ServiceReservations, causal::CausalServicePacketV1};
use crate::palw_state_v2::PalwBondKeyV2;
use crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1;
use borsh::BorshSerialize;

const PART: u64 = PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize)]
enum Stage {
    Begin,
    Part(u64),
    Finish,
}

/// An exact signed delivery branch of a previously accepted demand. Original
/// admission must separately reserve capacity for the maximum native branch.
pub(crate) struct MaterialBatchDeliveryV1 {
    id: Hash64,
    request_root: Hash64,
    release_daa: u64,
    service_before: u64,
    claim: Hash64,
    scope: Hash64,
    target: MaterialTargetV1,
    signer: PalwBondKeyV2,
    total: u64,
    body_hash: Hash64,
    draft: Hash64,
}

impl MaterialBatchDeliveryV1 {
    /// Resolve the exact target from the already accepted demand before
    /// assigning a delivery. The caller loads the notice and demand from the
    /// same branch and derives the bound from the original source.
    pub(crate) fn from_accepted_demand_v1(
        context: &crate::palw_court_shared_tir_state_v1::SharedTirContextV1,
        demand: &crate::palw_batch_court_v1::BatchDemandV1,
        declaration: &SignedMaterialIntakeV1,
        native_maximum: u64,
    ) -> Result<Self, String> {
        use crate::palw_batch_court_v1::BatchRangeAuthenticationV1;
        let request_root = demand.checked_request_root_v1().map_err(|err| format!("Batch demand: {err:?}"))?;
        if declaration.claim != context.claim
            || declaration.scope_id != context.scope_id
            || demand.case_window.claim_accepted_daa != context.original_accepted_daa
            || demand.case_window.material_until_daa != context.material_until_daa
            || demand.response_deadline_daa > context.hard_daa
            || u64::from(demand.part_bytes) != PART
        {
            return Err("Batch material demand substitutes the original context or part width".into());
        }
        let target = declaration.target.id_v1(context.claim)?;
        let range = demand
            .ranges
            .binary_search_by_key(&target, |range| range.id)
            .ok()
            .map(|index| &demand.ranges[index])
            .ok_or("Batch material target is outside the accepted demand")?;
        let release_daa = demand.opened_daa.checked_add(1).ok_or("Batch demand opening overflows")?;
        let delivery = Self::new(request_root, declaration, native_maximum, release_daa, demand.response_deadline_daa)?;
        if range.authentication != BatchRangeAuthenticationV1::WholeRange || delivery.total != u64::from(range.bytes) {
            return Err("Batch material declaration differs from the accepted range".into());
        }
        Ok(delivery)
    }

    fn new(
        request_root: Hash64,
        declaration: &SignedMaterialIntakeV1,
        native_maximum: u64,
        release_daa: u64,
        service_before: u64,
    ) -> Result<Self, String> {
        let Action::Begin { total_bytes, body_hash } = declaration.action else {
            return Err("Batch delivery requires a declaration".into());
        };
        if declaration.version != MATERIAL_BATCH_VERSION
            || request_root == Hash64::default()
            || release_daa >= service_before
            || declaration.claim == Hash64::default()
            || declaration.scope_id == Hash64::default()
            || total_bytes == 0
            || total_bytes > native_maximum.min(MATERIAL_PIPELINED_MAX_BYTES)
        {
            return Err("Batch declaration outside native demand bounds".into());
        }
        let draft = material_batch_draft_id_v1(
            request_root,
            declaration.claim,
            declaration.target,
            declaration.submitter,
            total_bytes,
            body_hash,
        )?;
        let id = crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/material-batch-delivery/v1",
            &(request_root, declaration.scope_id, draft),
        )?;
        Ok(Self {
            id,
            request_root,
            release_daa,
            service_before,
            claim: declaration.claim,
            scope: declaration.scope_id,
            target: declaration.target,
            signer: declaration.submitter,
            total: total_bytes,
            body_hash,
            draft,
        })
    }

    pub(crate) fn request_root_v1(&self) -> Hash64 {
        self.request_root
    }

    fn packet_id(&self, stage: Stage) -> Result<[u8; 64], String> {
        Ok(crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/material-batch-packet/v1", &(self.id, stage))?
            .as_bytes())
    }

    /// Include the signed Borsh envelope, even for the Begin and Finish
    /// controls. The native profile adds carrier, parser, signature and IO costs.
    pub(crate) fn service_graph_v1(
        &self,
        release_daa: u64,
        service_before: u64,
        mut cost: impl FnMut(u64) -> Result<ServiceCost, String>,
    ) -> Result<Vec<CausalServicePacketV1>, String> {
        if release_daa < self.release_daa || service_before > self.service_before || release_daa >= service_before {
            return Err("Batch delivery service interval is outside its accepted demand".into());
        }
        let parts = self.total.div_ceil(PART);
        let mut graph = Vec::with_capacity(parts as usize + 2);
        let mut append = |stage, action, payload, predecessors| -> Result<(), String> {
            let envelope = SignedMaterialIntakeV1 {
                version: MATERIAL_BATCH_VERSION,
                claim: self.claim,
                scope_id: self.scope,
                target: self.target,
                submitter: self.signer,
                action,
                signature: Vec::new(),
            };
            let minimum = borsh::object_length(&envelope).map_err(|e| e.to_string())? as u64
                + crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64
                + payload;
            let cost = cost(minimum)?;
            if cost.bytes < minimum || cost.controls != 1 || cost.work == 0 {
                return Err("Batch delivery service cost omits signed control".into());
            }
            graph.push(CausalServicePacketV1 {
                packet: ServicePacket { id: self.packet_id(stage)?, release_daa, service_before, cost },
                predecessors,
            });
            Ok(())
        };
        let begin = self.packet_id(Stage::Begin)?;
        append(Stage::Begin, Action::Begin { total_bytes: self.total, body_hash: self.body_hash }, 0, Vec::new())?;
        let mut predecessors = Vec::with_capacity(parts as usize);
        for index in 0..parts {
            let offset = index * PART;
            let stage = Stage::Part(offset);
            append(
                stage,
                Action::Part { draft_id: self.draft, offset, bytes: Vec::new() },
                PART.min(self.total - offset),
                vec![begin],
            )?;
            predecessors.push(self.packet_id(stage)?);
        }
        append(Stage::Finish, Action::Finish { draft_id: self.draft }, 0, predecessors)?;
        Ok(graph)
    }

    /// Match the actual signed action to its case-specific booked slot. The
    /// accepting caller must take the book from the live claim binding, and
    /// still verify the signature, native body and actual predecessor rows.
    pub(crate) fn check_slot_v1(
        &self,
        book: &ServiceReservations,
        daa: u64,
        signed: &SignedMaterialIntakeV1,
    ) -> Result<[u8; 64], String> {
        if (signed.version, signed.claim, signed.scope_id, signed.target, signed.submitter)
            != (MATERIAL_BATCH_VERSION, self.claim, self.scope, self.target, self.signer)
        {
            return Err("Batch material action has a foreign scope or signer".into());
        }
        let stage = match &signed.action {
            Action::Begin { total_bytes, body_hash } if (*total_bytes, *body_hash) == (self.total, self.body_hash) => Stage::Begin,
            Action::Part { draft_id, offset, bytes }
                if *draft_id == self.draft
                    && offset.is_multiple_of(PART)
                    && *offset < self.total
                    && bytes.len() as u64 == PART.min(self.total - *offset) =>
            {
                Stage::Part(*offset)
            }
            Action::Finish { draft_id } if *draft_id == self.draft => Stage::Finish,
            _ => return Err("Batch material action differs from its declaration".into()),
        };
        let id = self.packet_id(stage)?;
        if !(self.release_daa..self.service_before).contains(&daa) || book.assigned_daa(&id) != Some(daa) {
            return Err("Batch material action has no assigned service at this DAA".into());
        }
        let bytes = borsh::object_length(signed).map_err(|e| e.to_string())? as u64;
        if book.packet(&id).is_none_or(|packet| packet.cost.bytes < bytes) {
            return Err("Batch material action exceeds its booked signed bytes".into());
        }
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::TransactionOutpoint;

    #[test]
    fn accepted_demand_fixes_native_range_and_delivery_window() {
        use crate::palw_batch_court_v1::{BatchCourtWindowV1, BatchDemandV1, BatchRangeAuthenticationV1, BatchRangeV1};
        use crate::palw_court_shared_tir_state_v1::SharedTirContextV1;
        let h = Hash64::from_u64_word;
        let action = SignedMaterialIntakeV1 {
            version: MATERIAL_BATCH_VERSION,
            claim: h(1),
            scope_id: h(2),
            target: MaterialTargetV1::Unit { ordinal: 7 },
            submitter: PalwBondKeyV2(TransactionOutpoint::new(h(3), 0)),
            action: Action::Begin { total_bytes: PART + 1, body_hash: h(4) },
            signature: Vec::new(),
        };
        let context = SharedTirContextV1 {
            version: 1,
            claim: action.claim,
            allocation_id: h(5),
            source_id: h(6),
            rules_id: h(7),
            profile_id: h(8),
            scope_id: action.scope_id,
            queries: 8,
            original_accepted_daa: 2,
            hard_daa: 10_002,
            material_until_daa: 11_000,
        };
        let mut demand = BatchDemandV1 {
            case_id: h(9),
            case_window: BatchCourtWindowV1 {
                claim_accepted_daa: 2,
                challenge_opened_daa: 7,
                collect_until_daa: 20,
                material_until_daa: 11_000,
            },
            opened_daa: 10,
            response_deadline_daa: 14,
            maximum_bytes: PART + 1,
            part_bytes: PART as u32,
            ranges: vec![BatchRangeV1 {
                id: action.target.id_v1(action.claim).unwrap(),
                bytes: (PART + 1) as u32,
                authentication: BatchRangeAuthenticationV1::WholeRange,
            }],
        };
        let delivery = MaterialBatchDeliveryV1::from_accepted_demand_v1(&context, &demand, &action, PART + 1).unwrap();
        assert_eq!(delivery.request_root_v1(), demand.checked_request_root_v1().unwrap());
        assert!(MaterialBatchDeliveryV1::from_accepted_demand_v1(&context, &demand, &action, PART).is_err());
        let mut wrong_context = context.clone();
        wrong_context.material_until_daa += 1;
        assert!(MaterialBatchDeliveryV1::from_accepted_demand_v1(&wrong_context, &demand, &action, PART + 1).is_err());
        demand.ranges[0].authentication = BatchRangeAuthenticationV1::IndependentParts;
        assert!(MaterialBatchDeliveryV1::from_accepted_demand_v1(&context, &demand, &action, PART + 1).is_err());
        demand.ranges[0].authentication = BatchRangeAuthenticationV1::WholeRange;
        demand.ranges[0].id = h(11);
        assert!(MaterialBatchDeliveryV1::from_accepted_demand_v1(&context, &demand, &action, PART + 1).is_err());
    }

    #[test]
    fn batch_signature_and_draft_refuse_cross_request_reuse() {
        let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([81; 32]);
        let network = Hash64::from_u64_word(9);
        let request = Hash64::from_u64_word(5);
        let other_request = Hash64::from_u64_word(6);
        let hash = Hash64::from_u64_word(4);
        let mut action = SignedMaterialIntakeV1 {
            version: MATERIAL_BATCH_VERSION,
            claim: Hash64::from_u64_word(1),
            scope_id: Hash64::from_u64_word(2),
            target: MaterialTargetV1::Unit { ordinal: 7 },
            submitter: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(3), 0)),
            action: Action::Begin { total_bytes: 1, body_hash: hash },
            signature: Vec::new(),
        };
        let own = MaterialBatchDeliveryV1::new(request, &action, 1, 11, 14).unwrap();
        let foreign = MaterialBatchDeliveryV1::new(other_request, &action, 1, 11, 14).unwrap();
        assert_ne!(own.draft, foreign.draft);
        use super::super::MaterialDraftV1;
        use crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1;
        let retained = MaterialDraftV1 {
            version: action.version,
            claim: action.claim,
            scope_id: action.scope_id,
            target: action.target,
            submitter: action.submitter,
            draft_id: own.draft,
            total_bytes: 1,
            body_hash: hash,
            begun_block: Hash64::from_u64_word(10),
            begun_daa: 11,
            last_daa: 11,
            accepted_bytes: 0,
            head: request,
            expires_daa: 20,
            material_until_daa: 25,
        };
        let draft_key = PalwStagedCourtKeyV1::CourtMaterialDraft(action.claim, own.draft);
        let mut restored: MaterialDraftV1 = borsh::from_slice(&borsh::to_vec(&retained).unwrap()).unwrap();
        assert_eq!(restored, retained);
        assert!(restored.check(&draft_key).is_ok());
        restored.head = other_request;
        assert!(restored.check(&draft_key).is_err());

        for stage in [
            action.action.clone(),
            Action::Part { draft_id: own.draft, offset: 0, bytes: vec![7] },
            Action::Finish { draft_id: own.draft },
        ] {
            action.action = stage;
            action.signature = libcrux_ml_dsa::ml_dsa_87::sign(
                &key.signing_key,
                action.batch_signing_message_v1(network, request).unwrap().as_byte_slice(),
                super::super::MATERIAL_BATCH_CONTEXT_V1,
                [82; 32],
            )
            .unwrap()
            .as_ref()
            .to_vec();
            assert!(action.verify_batch_v1(network, request, key.verification_key.as_ref()).is_ok());
            assert!(action.verify_batch_v1(network, other_request, key.verification_key.as_ref()).is_err());
            assert!(action.verify_v1(network, key.verification_key.as_ref()).is_err());
        }
    }

    #[test]
    fn funded_batch_delivery_binds_signed_fragments_and_fits_parallel_slots() {
        let total = PART + 1;
        let mut action = SignedMaterialIntakeV1 {
            version: MATERIAL_BATCH_VERSION,
            claim: Hash64::from_u64_word(1),
            scope_id: Hash64::from_u64_word(2),
            target: MaterialTargetV1::Unit { ordinal: 7 },
            submitter: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(3), 0)),
            action: Action::Begin { total_bytes: total, body_hash: Hash64::from_u64_word(4) },
            signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        let request = Hash64::from_u64_word(5);
        let delivery = MaterialBatchDeliveryV1::new(request, &action, total, 11, 14).unwrap();
        let cost = |bytes| Ok(ServiceCost { bytes, work: 1, io: 1, controls: 1 });
        let graph = delivery.service_graph_v1(11, 14, cost).unwrap();
        assert_eq!(graph.len(), 4);
        assert!(delivery.service_graph_v1(10, 14, cost).is_err());
        assert!(delivery.service_graph_v1(11, 15, cost).is_err());
        assert!(delivery.service_graph_v1(11, 14, |bytes| cost(bytes - 1)).is_err());
        let mut book = ServiceReservations::new(ServiceCost { bytes: PART * 2, work: 2, io: 2, controls: 2 }).unwrap();
        book.reserve_causal_v1(&graph, 14).unwrap();
        assert_eq!(delivery.check_slot_v1(&book, 11, &action).unwrap(), graph[0].packet.id);
        assert!(delivery.check_slot_v1(&book, 12, &action).is_err());
        let other = MaterialBatchDeliveryV1::new(Hash64::from_u64_word(6), &action, total, 11, 14).unwrap();
        assert!(other.check_slot_v1(&book, 11, &action).is_err());
        for (index, (offset, len)) in [(0, PART), (PART, 1)].into_iter().enumerate().rev() {
            action.action = Action::Part { draft_id: delivery.draft, offset, bytes: vec![0; len as usize] };
            assert_eq!(delivery.check_slot_v1(&book, 12, &action).unwrap(), graph[index + 1].packet.id);
            assert_eq!(borsh::object_length(&action).unwrap() as u64, graph[index + 1].packet.cost.bytes);
        }
        action.action = Action::Part { draft_id: Hash64::from_u64_word(8), offset: 0, bytes: vec![0; PART as usize] };
        assert!(delivery.check_slot_v1(&book, 12, &action).is_err());
        action.action = Action::Finish { draft_id: delivery.draft };
        assert!(delivery.check_slot_v1(&book, 12, &action).is_err());
        assert_eq!(delivery.check_slot_v1(&book, 13, &action).unwrap(), graph[3].packet.id);
        action.submitter = PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(3), 1));
        assert!(delivery.check_slot_v1(&book, 13, &action).is_err());
    }
}
