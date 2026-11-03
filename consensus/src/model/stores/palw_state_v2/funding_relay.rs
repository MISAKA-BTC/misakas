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
    assignment::funding_relay::{FUNDING_RELAY_MAX_BODY_BYTES_V2, FundingRelayRegistrationBodyV2},
    identity::Digest,
    native::KERNEL_AUX_TABLE,
    rows::{NativeRowBody, NativeRowValue},
};
use rocksdb::WriteBatch;

const RELAY_MATERIAL_SUFFIX: u8 = 0x4c;

fn key(id: Digest) -> [u8; 66] {
    let mut bytes = [0; 66];
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

pub(crate) fn has_new_funding_relay_rows_v2(delta: &PalwStateDeltaV2) -> bool {
    delta.entries.iter().any(|entry| created_relay_row(entry).is_some())
}

impl DbPalwStateV2Store {
    /// The caller holds the PALW archive lock and the admitted carrier owner
    /// through the enclosing batch commit. Reads and checks precede all puts.
    pub(crate) fn stage_funding_relay_material_v2(
        &self,
        batch: &mut WriteBatch,
        delta: &PalwStateDeltaV2,
        carriers: &[PositiveAssignmentScheduledActionV2],
    ) -> StoreResult<Option<NativeStoreWriteTicketV1>> {
        let count = delta.entries.iter().filter(|entry| created_relay_row(entry).is_some()).count();
        if count == 0 {
            return Ok(None);
        }
        if count % 2 != 0 || count / 2 > carriers.len() {
            return Err(invalid("new rows lack complete carrier pairs"));
        }
        type Record<'a> = ([u8; 66], &'a [u8]);
        let count = count / 2;
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
        let mut entries = delta.entries.iter();
        while let Some(entry) = entries.next() {
            let Some((auth_key, auth_bytes)) = created_relay_row(entry) else { continue };
            let (custody_key, custody_bytes) =
                entries.next().and_then(created_relay_row).ok_or_else(|| invalid("new authorization lacks adjacent custody"))?;
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
            let index = carriers
                .binary_search_by_key(&origin.accepted_tx_ordinal, |carrier| carrier.action.origin().accepted_tx_ordinal)
                .map_err(|_| invalid("missing actual carrier at registration ordinal"))?;
            let action = &carriers[index].action;
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
            let fees = action.relay_carrier_fees_v2().ok_or_else(|| invalid("missing actual carrier fee partition"))?;
            let charge =
                auth.carrier_charge_material(fees).map_err(|_| invalid("credited tip does not account for the quoted charge"))?;
            if charge != auth.metadata_charge {
                return Err(invalid("actual charge differs from the canonical registration reference"));
            }
            let key = key(auth.material.digest);
            if let Some(saved) = self.db.get_pinned(key)? {
                if saved.as_ref() != payload {
                    return Err(invalid("content address already contains different bytes"));
                }
                continue;
            }
            // RocksDB Put: type, one-byte key length, key, length varint,
            // and value. A valid op14 payload takes exactly two varint bytes.
            let bytes = (payload.len() + key.len() + 4) as u64;
            if payload.len() > FUNDING_RELAY_MAX_BODY_BYTES_V2 + 3 || bytes > auth.quote.usage.batch_bytes {
                return Err(invalid("literal material exceeds its quoted batch footprint"));
            }
            added = added.checked_add(bytes).ok_or_else(|| capacity("batch extent overflow"))?;
            pending.push((key, payload));
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
        for (key, payload) in pending {
            batch.put(key, payload);
        }
        debug_assert_eq!(batch.size_in_bytes() as u64, total);
        Ok(Some(NativeStoreWriteTicketV1 { _lease: Some(lease) }))
    }
}
