//! Immutable RFC-0046 payload custody. The owning UTXO commit stages the
//! literal carrier bytes with their matching canonical registration delta.
use super::{DbPalwStateV2Store, NativeStoreWriteTicketV1};
use kaspa_consensus_core::{
    palw_native_resource_v1::{actual_hard_database_quota_v1, reserve_native_requests_v1},
    palw_state_v2::{PalwDeltaEntryV2, PalwStateDeltaV2, PositiveAssignmentScheduledActionV2},
    subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
};
use kaspa_database::{prelude::*, registry::DatabaseStorePrefixes};
use misaka_palw_positive_contract::{
    assignment::{
        funding_relay::FundingRelayRegistrationBodyV2,
        funding_relay_policy::{FUNDING_RELAY_MATERIAL_KEY_BYTES_V2, FundingRelayStorageExtentV2},
        funding_relay_rows::{FUNDING_RELAY_CHARGE_BYTES_V2, FundingRelayAuthorizationRowV2, FundingRelayCarrierFeesV2},
    },
    identity::Digest,
    native::KERNEL_AUX_TABLE,
    rows::{NativeRowBody, NativeRowValue},
};
use rocksdb::WriteBatch;

const RELAY_MATERIAL_SUFFIX: u8 = 0x4c;

fn key(id: Digest) -> [u8; FUNDING_RELAY_MATERIAL_KEY_BYTES_V2] {
    let mut bytes = [0; FUNDING_RELAY_MATERIAL_KEY_BYTES_V2];
    bytes[0] = u8::from(DatabaseStorePrefixes::PalwStateV2Tip);
    bytes[1] = RELAY_MATERIAL_SUFFIX;
    bytes[2..].copy_from_slice(&id.0);
    bytes
}

fn invalid(why: &str) -> StoreError {
    StoreError::DataInconsistency(format!("funding relay material: {why}"))
}

fn capacity(why: &str) -> StoreError {
    StoreError::LocalResourceUnavailable(format!("funding relay material: {why}"))
}

fn created_relay_row(entry: &PalwDeltaEntryV2) -> Option<(&[u8], &[u8])> {
    match entry {
        PalwDeltaEntryV2::KernelRouteRow { table, key, old: None, new: Some(value) }
            if *table == KERNEL_AUX_TABLE && matches!(key.get(4), Some(19 | 20)) =>
        {
            Some((key, value))
        }
        _ => None,
    }
}

enum StoredRelayValue<'a> {
    Payload(&'a [u8]),
    Charge([u8; FUNDING_RELAY_CHARGE_BYTES_V2]),
}

impl StoredRelayValue<'_> {
    fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Payload(raw) => raw,
            Self::Charge(raw) => raw,
        }
    }
}

impl DbPalwStateV2Store {
    /// Verify the archived fee partition against the rooted authorization.
    /// The caller must separately establish accepted ancestry and custody.
    fn read_funding_relay_charge_v2(&self, auth: &FundingRelayAuthorizationRowV2) -> StoreResult<Option<FundingRelayCarrierFeesV2>> {
        let Some(raw) = self.db.get_pinned(key(auth.metadata_charge.digest))? else { return Ok(None) };
        let fees = FundingRelayCarrierFeesV2::from_archive_bytes(&raw).map_err(|_| invalid("invalid archived fee partition"))?;
        if auth.carrier_charge_material(fees).ok() != Some(auth.metadata_charge) {
            return Err(invalid("archived charge does not match authorization"));
        }
        Ok(Some(fees))
    }

    /// Read the exact payload and charge selected by one rooted row. A
    /// separate native caller checks that the original acceptance is on its
    /// selected chain. Presence here cannot grant custody or fee credit.
    pub(crate) fn check_funding_relay_archive_v2(
        &self,
        auth: &FundingRelayAuthorizationRowV2,
        matches_payload: impl FnOnce(&[u8]) -> bool,
    ) -> StoreResult<FundingRelayCarrierFeesV2> {
        let payload =
            self.db.get_pinned(key(auth.material.digest))?.ok_or_else(|| capacity("rooted literal relay payload unavailable"))?;
        if !matches_payload(payload.as_ref()) {
            return Err(invalid("archived literal relay payload differs"));
        }
        self.read_funding_relay_charge_v2(auth)?.ok_or_else(|| capacity("rooted relay charge unavailable"))
    }

    /// The caller holds the PALW archive lock and the admitted carrier owner
    /// through the enclosing batch commit. Reads and checks precede all puts.
    pub(crate) fn stage_funding_relay_material_v2(
        &self,
        batch: &mut WriteBatch,
        delta: &PalwStateDeltaV2,
        carriers: &[PositiveAssignmentScheduledActionV2],
    ) -> StoreResult<Option<NativeStoreWriteTicketV1>> {
        let count = delta.entries.iter().filter(|entry| created_relay_row(entry).is_some()).count();
        let mut charged = carriers.iter().filter_map(|carrier| {
            let fees = carrier.action.relay_carrier_fees_v2()?;
            (fees.relay_charge() != 0).then_some((&carrier.action, fees))
        });
        if count == 0 {
            if charged.next().is_some() {
                return Err(invalid("native relay debit has no registration rows"));
            }
            return Ok(None);
        }
        if count % 2 != 0 || count / 2 > carriers.len() {
            return Err(invalid("new rows lack complete carrier pairs"));
        }
        type Record<'a> = ([u8; FUNDING_RELAY_MATERIAL_KEY_BYTES_V2], StoredRelayValue<'a>);
        // Two created canonical rows correspond to at most two archive puts.

        let scratch = (size_of::<Record<'_>>() as u64)
            .checked_mul(count as u64)
            .and_then(|n| n.checked_add((4 * size_of::<NativeRowValue>() + 2 * size_of::<FundingRelayRegistrationBodyV2>()) as u64))
            .ok_or_else(|| capacity("staging workspace overflow"))?;
        let mut lease = reserve_native_requests_v1(scratch)
            .map_err(|why| capacity(&why.to_string()))?
            .ok_or_else(|| capacity("native write workspace owner is not armed"))?;
        let mut pending: Vec<Record<'_>> = Vec::new();
        pending.try_reserve_exact(count).map_err(|_| capacity("staging record slots"))?;
        if pending.capacity() > count {
            return Err(capacity("staging record slots exceeded reservation"));
        }
        let mut added = 0u64;
        let mut previous_ordinal = None;
        let mut entries = delta.entries.iter();
        while let Some(entry) = entries.next() {
            let Some((auth_key, auth_bytes)) = created_relay_row(entry) else { continue };
            let custody_entry = entries.next().ok_or_else(|| invalid("new authorization lacks adjacent custody"))?;
            let (custody_key, custody_bytes) =
                created_relay_row(custody_entry).ok_or_else(|| invalid("new authorization lacks adjacent custody"))?;
            let NativeRowBody::FundingRelayAuthorization(auth) =
                NativeRowValue::decode(auth_key, auth_bytes).map_err(|_| invalid("invalid authorization row"))?.body
            else {
                return Err(invalid("expected new authorization row"));
            };
            let NativeRowBody::FundingRelayCustody(custody) =
                NativeRowValue::decode(custody_key, custody_bytes).map_err(|_| invalid("invalid custody row"))?.body
            else {
                return Err(invalid("expected new custody row"));
            };
            custody.check_pair(&auth).map_err(|_| invalid("new registration pair disagrees"))?;
            let origin = auth.origin;
            if custody.binding.is_some()
                || origin.accepting_block.0 != delta.point.block.as_bytes()
                || origin.accepted_daa != delta.point.daa_score
                || origin.accepted_blue_score != delta.point.blue_score
            {
                return Err(invalid("new registration has an incorrect accepting point or binding"));
            }
            let (action, fees) = charged.next().ok_or_else(|| invalid("registration has no observed native debit"))?;
            if previous_ordinal.is_some_and(|previous| origin.accepted_tx_ordinal <= previous) {
                return Err(invalid("registration ordinals are not strictly increasing"));
            }
            previous_ordinal = Some(origin.accepted_tx_ordinal);
            let tx = action.transaction();
            if action.origin() != origin
                || tx.id().as_bytes() != origin.carrier_transaction.0
                || tx.subnetwork_id != SUBNETWORK_ID_PALW_LIFECYCLE
                || !tx.payload.starts_with(&[1, 0, 176])
            {
                return Err(invalid("actual lifecycle carrier differs from registration origin"));
            }
            let payload = &tx.payload[..];
            let body = FundingRelayRegistrationBodyV2::decode(&payload[3..], auth.physical.max_frame_bytes)
                .map_err(|_| invalid("invalid bounded literal carrier"))?;
            if body.terms != auth.terms
                || body.purpose_signature != auth.purpose_signature
                || body.relay_signature != custody.relay_signature
            {
                return Err(invalid("carrier differs from both retained signatures and terms"));
            }
            let charge =
                auth.carrier_charge_material(fees).map_err(|_| invalid("credited tip does not account for the quoted charge"))?;
            if charge != auth.metadata_charge {
                return Err(invalid("actual charge differs from the canonical registration reference"));
            }
            let material_key = key(auth.material.digest);
            let charge_key = key(charge.digest);
            if material_key == charge_key {
                return Err(invalid("charge and payload share a content address"));
            }
            let auth_extent = borsh::object_length(entry).map_err(|_| invalid("authorization delta size"))? as u64;
            let custody_extent = borsh::object_length(custody_entry).map_err(|_| invalid("custody delta size"))? as u64;
            let aux_extent = (auth_key.len() + auth_bytes.len() + custody_key.len() + custody_bytes.len()) as u64;
            let delta_extent =
                auth_extent.checked_add(custody_extent).ok_or_else(|| capacity("registration delta extent overflow"))?;
            let measured = FundingRelayStorageExtentV2::from_encoded(payload.len() as u64, aux_extent, delta_extent)
                .map_err(|_| invalid("invalid encoded registration extent"))?;
            let expected =
                FundingRelayStorageExtentV2::for_terms(&auth.terms).map_err(|_| invalid("invalid registration quote extent"))?;
            if measured != expected {
                return Err(invalid("encoded registration extent differs from preflight"));
            }
            let quote = auth
                .fees
                .registration_quote(&auth.policy, &auth.physical, &auth.terms, origin.accepted_daa)
                .map_err(|_| invalid("invalid original registration quote"))?;
            if quote != auth.quote {
                return Err(invalid("registration differs from the native reservation quote"));
            }
            if let Some(saved) = self.db.get_pinned(material_key)? {
                if saved.as_ref() != payload {
                    return Err(invalid("content address already contains different bytes"));
                }
            } else {
                added = added.checked_add(measured.material_put_bytes).ok_or_else(|| capacity("batch extent overflow"))?;
                pending.push((material_key, StoredRelayValue::Payload(payload)));
            }

            if let Some(saved) = self.read_funding_relay_charge_v2(&auth)? {
                if saved != fees {
                    return Err(invalid("observed carrier fees differ from archived partition"));
                }
            } else {
                added = added.checked_add(measured.charge_put_bytes).ok_or_else(|| capacity("batch extent overflow"))?;
                pending.push((charge_key, StoredRelayValue::Charge(fees.archive_bytes())));
            }
        }
        if charged.next().is_some() {
            return Err(invalid("native relay debit has no registration rows"));
        }
        if pending.is_empty() {
            return Ok(None);
        }
        pending.sort_unstable_by_key(|(key, _)| *key);
        if pending.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(invalid("duplicate material put in one native batch"));
        }
        let db_path = self.db.native_single_directory_quota_path_v1().ok_or_else(|| capacity("database quota provenance missing"))?;
        let quota = actual_hard_database_quota_v1(db_path).map_err(|why| capacity(&why.to_string()))?;
        let growth = added.checked_mul(2).ok_or_else(|| capacity("WAL and batch extent overflow"))?;
        if growth > quota.headroom_bytes() {
            return Err(capacity("insufficient actual database quota headroom"));
        }
        // Keep the batch's coexisting old/new buffer allocation owned until
        // commit or abandonment, like the other native archive writers.
        let before = batch.size_in_bytes() as u64;
        let total = before.checked_add(added).ok_or_else(|| capacity("total batch extent overflow"))?;
        lease.grow_memory_v1(total).map_err(|why| capacity(&why.to_string()))?;
        for (key, value) in pending {
            batch.put(key, value.as_bytes());
        }
        debug_assert_eq!(batch.size_in_bytes() as u64, total);
        Ok(Some(NativeStoreWriteTicketV1 { _lease: Some(lease) }))
    }
}
