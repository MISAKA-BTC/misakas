//! Explicitly funded private ACK carrier adapter. A transaction is persisted
//! before submission. Only the current finalized, actually applied provider
//! checkpoint retires its outbox; transaction inclusion alone never does.
use super::native_provider::LoopbackNativeProviderV3;
use crate::palw_native_custody_v3::PalwNativeCustodyContextV3;
use kaspa_consensus_core::{
    palw_lifecycle_objects_v2::PALW_LIFECYCLE_TX_VERSION_V2,
    palw_native_resource_v1::{NativeDemandLeaseV1, NativeRetainedV1, reserve_native_requests_v1},
    palw_state_v2::PalwConsensusObjectV2,
    tx::{Transaction, TransactionOutpoint},
};
use std::{collections::HashSet, path::Path, sync::Arc, time::Duration};

/// A durable, already signed additional object. The provider service owns
/// purpose-specific acceptance/finality checks and outbox publication.
pub(crate) struct AdditionalNativeCarrierV4 {
    pub id: kaspa_consensus_core::Hash64,
    pub object: NativeRetainedV1<PalwConsensusObjectV2>,
    pub submitted_transaction: Option<NativeRetainedV1<Vec<u8>>>,
}

enum PendingCarrierV4 {
    Ack(
        kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        misaka_palw_remote::material_recovery_ack_v2::MaterialRecoveryAckSummaryV2,
        NativeRetainedV1<PalwConsensusObjectV2>,
        Option<NativeRetainedV1<Vec<u8>>>,
    ),
    Additional(AdditionalNativeCarrierV4),
}
impl PendingCarrierV4 {
    fn object(&self) -> &PalwConsensusObjectV2 {
        match self {
            Self::Ack(_, _, object, _) => object,
            Self::Additional(row) => &row.object,
        }
    }
    fn submitted(&self) -> Option<&[u8]> {
        match self {
            Self::Ack(_, _, _, transaction) => transaction.as_ref().map(|bytes| bytes.as_slice()),
            Self::Additional(row) => row.submitted_transaction.as_ref().map(|bytes| bytes.as_slice()),
        }
    }
    fn record_submitted(&self, service: &LoopbackNativeProviderV3, tx: &Transaction) -> Result<(), String> {
        match self {
            Self::Ack(provider, row, _, _) => service.record_recovery_ack_submitted_v2(*provider, row.receipt, tx),
            Self::Additional(row) => service.record_additional_carrier_submitted_v4(row.id, tx),
        }
    }
}

pub struct PrivateRecoveryAckFeeSpecV4 {
    keypair: libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
    initial: Vec<TransactionOutpoint>,
    maximum_fee_sompi: u64,
}
impl PrivateRecoveryAckFeeSpecV4 {
    pub fn parse(key: Option<&str>, funding: Option<&str>, fee_cap: Option<&str>) -> Result<Option<Self>, String> {
        if key.is_none() && funding.is_none() && fee_cap.is_none() {
            return Ok(None);
        }
        let key = key.ok_or("recovery ACK needs an explicit payer key")?;
        if !Path::new(key).is_absolute() {
            return Err("recovery ACK payer key path must be absolute".into());
        }
        let maximum_fee_sompi: u64 =
            fee_cap.ok_or("recovery ACK needs an explicit fee cap")?.parse().map_err(|_| "recovery ACK fee cap must be u64")?;
        if maximum_fee_sompi == 0 {
            return Err("recovery ACK fee cap must be positive".into());
        }
        let initial = funding
            .ok_or("recovery ACK needs explicit initial funding outpoints")?
            .split(',')
            .map(crate::palw_producer::parse_outpoint)
            .collect::<Result<Vec<_>, _>>()?;
        if initial.is_empty()
            || initial.len() > 64
            || initial.iter().collect::<HashSet<_>>().len() != initial.len()
            || initial.iter().any(|o| kaspa_consensus_core::config::drill::palw_t12_public_genesis_txid_v1(&o.transaction_id))
        {
            return Err("recovery ACK funding is duplicate, excessive or names a public genesis input".into());
        }
        let keypair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair(kaspa_pq_validator_core::load_validator_seed(key)?);
        Ok(Some(Self { keypair, initial, maximum_fee_sompi }))
    }

    pub(super) fn start(self, service: Arc<LoopbackNativeProviderV3>, ctx: &PalwNativeCustodyContextV3) -> Result<(), String> {
        if ctx.config.params.palw_material_release_recovery_v2.is_none() {
            return Err("recovery ACK adapter requires the new explicit recovery profile".into());
        }
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
        let manager = ctx.consensus_manager.clone();
        let config = ctx.config.clone();
        let flow = ctx.flow_context.clone();
        let tick = ctx.tick_service.clone();
        std::thread::Builder::new()
            .name("palw-private-recovery-ack-v4".into())
            .spawn(move || {
                runtime.block_on(async move {
                    let script = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(
                        &kaspa_hashes::blake2b_512_address_payload(self.keypair.verification_key.as_ref()).as_bytes(),
                    );
                    let mut cursor = None;
                    let mut prefer_additional = false;
                    loop {
                        if matches!(tick.tick(Duration::from_secs(2)).await, kaspa_core::task::tick::TickReason::Shutdown) {
                            break;
                        }
                        let pending = match service.pending_recovery_ack_v2() {
                            Ok(rows) => rows,
                            Err(e) => {
                                kaspa_core::warn!("private ACK outbox: {e}");
                                continue;
                            }
                        };
                        // Service-side bounded cursors rotate providers and rows;
                        // no all-outbox deep copies precede resource admission.
                        let selected = pending.and_then(|(provider, row)| {
                            match service.retire_recovery_ack_checkpoint_v2(provider, row.receipt) {
                                Ok(true) => None,
                                Ok(false) => Some((provider, row)),
                                Err(e) => {
                                    kaspa_core::warn!("private ACK finalized checkpoint: {e}");
                                    None
                                }
                            }
                        });
                        let ack =
                            selected.and_then(|(provider, row)| match service.recovery_ack_object_owned_v2(provider, row.receipt) {
                                Ok(object) => match service.recovery_ack_transaction_owned_v2(provider, row.receipt) {
                                    Ok(transaction) => Some(PendingCarrierV4::Ack(provider, row, object, transaction)),
                                    Err(e) => {
                                        kaspa_core::warn!("private ACK transaction reservation: {e}");
                                        None
                                    }
                                },
                                Err(e) => {
                                    kaspa_core::warn!("private ACK object reservation: {e}");
                                    None
                                }
                            });
                        let additional = match service.pending_additional_carrier_v4() {
                            Ok(rows) => rows,
                            Err(e) => {
                                kaspa_core::warn!("private material carrier outbox: {e}");
                                None
                            }
                        };
                        let selected_additional =
                            additional.and_then(|row| match service.retire_additional_carrier_finalized_v4(row.id) {
                                Ok(true) => None,
                                Ok(false) => Some(PendingCarrierV4::Additional(row)),
                                Err(e) => {
                                    kaspa_core::warn!("private material carrier finalized state: {e}");
                                    None
                                }
                            });
                        let selected = if prefer_additional { selected_additional.or(ack) } else { ack.or(selected_additional) };
                        prefer_additional = !prefer_additional;
                        let Some(job) = selected else {
                            continue;
                        };
                        let session = manager.consensus().unguarded_session();
                        let object = job.object();
                        // Keep the constructor/decoder ticket through relay admission.
                        // Pool ownership and its own capacity are a separate boundary.
                        let (transaction, _transaction_lease) = if let Some(submitted) = job.submitted() {
                            match decode_carrier(submitted, object) {
                                Ok(tx) => tx,
                                Err(e) => {
                                    kaspa_core::warn!("private ACK retained carrier: {e}");
                                    continue;
                                }
                            }
                        } else {
                            let daa = session.get_virtual_daa_score();
                            let minimum =
                                crate::palw_panel::palw_carrier_fees::palw_lifecycle_funding_minimum_v2(&config.params, object, daa);
                            let locked: HashSet<_> = session.palw_locked_bond_outpoints_v2().into_iter().collect();
                            let usable = |outpoint: &TransactionOutpoint, entry: &kaspa_consensus_core::tx::UtxoEntry| {
                                entry.amount >= minimum
                                    && entry.script_public_key == script
                                    && !flow.mining_manager().outpoint_is_spent_in_mempool(outpoint)
                                    && crate::palw_panel::palw_fee_funding_usable_v1(
                                        outpoint,
                                        entry,
                                        daa,
                                        config.params.coinbase_spend_maturity(),
                                        None,
                                        &locked,
                                    )
                            };
                            let mut funding = self.initial.iter().find_map(|outpoint| {
                                session
                                    .get_virtual_utxo_entry(*outpoint)
                                    .filter(|entry| usable(outpoint, entry))
                                    .map(|entry| (*outpoint, entry))
                            });
                            // Bounded paging finds confirmed change after an outbox
                            // is retired, without an unbounded remembered-spend set.
                            if funding.is_none() {
                                let page = session.async_get_virtual_utxos(cursor, 1024, cursor.is_some()).await;
                                cursor = page.last().map(|(outpoint, _)| *outpoint);
                                if page.len() < 1024 {
                                    cursor = None;
                                }
                                funding = page.into_iter().find(|(outpoint, entry)| usable(outpoint, entry));
                            }
                            let Some((outpoint, entry)) = funding else {
                                continue;
                            };
                            let construction_lease =
                                match reserve_carrier_build_v4(object, &entry, self.keypair.verification_key.as_ref().len()) {
                                    Ok(lease) => lease,
                                    Err(e) => {
                                        kaspa_core::warn!("private material carrier construction capacity: {e}");
                                        continue;
                                    }
                                };
                            let tx = match crate::palw_panel::palw_carrier_fees::build_lifecycle_carrier_v2(
                                &config,
                                &self.keypair,
                                object,
                                outpoint,
                                &entry,
                                &[],
                                None,
                                daa,
                            ) {
                                Ok(tx) => tx,
                                Err(e) => {
                                    kaspa_core::warn!("private ACK funding: {e}");
                                    continue;
                                }
                            };
                            let output = tx.outputs.iter().try_fold(0u64, |sum, o| sum.checked_add(o.value));
                            let fee = output.and_then(|output| entry.amount.checked_sub(output));
                            if fee.is_none_or(|fee| fee > self.maximum_fee_sompi) {
                                kaspa_core::warn!("private ACK carrier exceeds its explicitly configured payer fee cap");
                                continue;
                            }
                            if let Err(e) = job.record_submitted(&service, &tx) {
                                kaspa_core::warn!("private ACK durable carrier: {e}");
                                continue;
                            }
                            (tx, construction_lease)
                        };
                        // A timeout/eviction/reorg keeps the same spend in the WAL.
                        // No local send result, clock or receipt absence clears it.
                        if flow
                            .mining_manager()
                            .clone()
                            .has_transaction(transaction.id(), kaspa_mining::model::tx_query::TransactionQuery::TransactionsOnly)
                            .await
                        {
                            continue;
                        }
                        match tokio::time::timeout(
                            Duration::from_secs(10),
                            flow.submit_rpc_transaction(&session, transaction, kaspa_mining::mempool::tx::Orphan::Forbidden),
                        )
                        .await
                        {
                            Ok(Ok(())) => {}
                            Ok(Err(e)) => kaspa_core::warn!("private ACK carrier remains pending after relay refusal: {e}"),
                            Err(_) => kaspa_core::warn!("private ACK relay timed out; its durable carrier remains pending"),
                        }
                    }
                });
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        Hash64,
        config::{Config, params::Params},
        network::{NetworkId, NetworkType},
        palw_material_release_recovery_v2::PalwMaterialRecoveryActionV2,
        palw_monetary_material_v1::PalwMonetaryMaterialActionV1,
        palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
        tx::UtxoEntry,
    };
    fn object(receipt: u64) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::MonetaryMaterialActionV1 {
            action: Box::new(PalwMonetaryMaterialActionV1::RecoveryV2(Box::new(PalwMaterialRecoveryActionV2::Acknowledge {
                root: Hash64::from_u64_word(1),
                receipt: Hash64::from_u64_word(receipt),
                stream: Hash64::from_u64_word(3),
                previous: Hash64::default(),
                sequence: 1,
            }))),
            signer: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(4), 0)),
            // This test checks carrier framing, not acceptance of an ACK.
            signature: vec![7; 32],
        }
    }
    #[test]
    fn bounded_decoder_matches_real_carrier_wire_and_refuses_substitution() {
        let config = Config::new(Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12)));
        let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([23; 32]);
        let script = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(
            &kaspa_hashes::blake2b_512_address_payload(key.verification_key.as_ref()).as_bytes(),
        );
        let funding = UtxoEntry::new(100_000_000, script, 0, false);
        let object = object(2);
        let tx = crate::palw_panel::palw_carrier_fees::build_lifecycle_carrier_v2(
            &config,
            &key,
            &object,
            TransactionOutpoint::new(Hash64::from_u64_word(5), 0),
            &funding,
            &[],
            None,
            0,
        )
        .unwrap();
        let bytes = borsh::to_vec(&tx).unwrap();
        assert_eq!(decode_carrier(&bytes, &object).unwrap().0, tx);
        let original = kaspa_consensus_core::palw_lifecycle_objects_v2::PalwLifecycleTxPayloadV2 {
            version: PALW_LIFECYCLE_TX_VERSION_V2,
            object: object.clone(),
        };
        assert_eq!(tx.payload, borsh::to_vec(&original).unwrap(), "borrowed serialization preserves existing carrier bytes");
        assert!(decode_carrier(&bytes, &self::object(9)).is_err());
        let mut corrupt = bytes.clone();
        corrupt[2..6].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_carrier(&corrupt, &object).is_err());
        let mut corrupt = bytes.clone();
        *corrupt.last_mut().unwrap() ^= 1;
        assert!(decode_carrier(&corrupt, &object).is_err(), "cached transaction ID is recomputed");
        for length in [0, 2, 6, 74, bytes.len() - 1] {
            assert!(decode_carrier(&bytes[..length], &object).is_err());
        }
    }
}

/// Exact single-input/single-change carrier grammar BEFORE Borsh allocates.
/// The payload is then compared to the already bounded signed outbox object;
/// no arbitrary consensus-object enum is decoded from a local corrupt frame.
pub(super) fn decode_carrier(
    bytes: &[u8],
    object: &PalwConsensusObjectV2,
) -> Result<(Transaction, Option<NativeDemandLeaseV1>), String> {
    struct Cursor<'a>(&'a [u8]);
    impl<'a> Cursor<'a> {
        fn take(&mut self, n: usize) -> Result<(), String> {
            self.0 = self.0.get(n..).ok_or("private ACK transaction truncated")?;
            Ok(())
        }
        fn count(&mut self, max: usize) -> Result<usize, String> {
            let word = self.0.get(..4).ok_or("private ACK transaction count truncated")?;
            let n = u32::from_le_bytes(word.try_into().unwrap()) as usize;
            self.take(4)?;
            if n > max {
                return Err("private ACK transaction nested capacity exceeded".into());
            }
            Ok(n)
        }
        fn blob(&mut self, max: usize) -> Result<usize, String> {
            let n = self.count(max)?;
            self.take(n)?;
            Ok(n)
        }
    }
    if bytes.len() > 64 * 1024 * 1024 {
        return Err("private ACK transaction exceeds framing cap".into());
    }
    let mut c = Cursor(bytes);
    c.take(2)?;
    if c.count(1)? != 1 {
        return Err("private ACK carrier must have one funding input".into());
    }
    c.take(68)?;
    let signature = c.blob(16 * 1024)?;
    c.take(9)?;
    if c.count(1)? != 1 {
        return Err("private ACK carrier must have one change output".into());
    }
    c.take(8 + 2)?;
    let script = c.blob(4096)?;
    c.take(8 + kaspa_consensus_core::subnets::SUBNETWORK_ID_SIZE + 8)?;
    let payload = c.blob(64 * 1024 * 1024)?;
    c.take(8 + 64)?;
    if !c.0.is_empty() {
        return Err("private ACK transaction has trailing bytes".into());
    }
    // These are the actual three byte-vector lengths from the validated
    // transaction grammar, not a raw-frame-to-heap conversion. Include the
    // nested input/output layouts and the SmallVec decode temporary separately.
    let lease = reserve_carrier_parts_v4(&[
        (std::mem::size_of::<Transaction>(), 1),
        (std::mem::size_of::<kaspa_consensus_core::tx::TransactionInput>(), 2),
        (std::mem::size_of::<kaspa_consensus_core::tx::TransactionOutput>(), 2),
        (signature, 2),
        (script, 4),
        (payload, 2),
    ])?;
    let mut tx: Transaction = borsh::from_slice(bytes).map_err(|e| e.to_string())?;
    let supplied = tx.id();
    tx.finalize();
    // Compare the borrowed object's canonical bytes in place. No deep object
    // clone or second payload vector is needed to authenticate the stored tx.
    struct Compare<'a>(&'a [u8]);
    impl std::io::Write for Compare<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.0.get(..bytes.len()) != Some(bytes) {
                return Err(std::io::Error::other("carrier signed object differs"));
            }
            self.0 = &self.0[bytes.len()..];
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut comparison = Compare(&tx.payload);
    borsh::to_writer(&mut comparison, &(PALW_LIFECYCLE_TX_VERSION_V2, object)).map_err(|e| e.to_string())?;
    if tx.id() != supplied
        || !comparison.0.is_empty()
        || tx.version != kaspa_consensus_core::constants::TX_VERSION
        || tx.subnetwork_id != kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE
        || tx.lock_time != 0
        || tx.gas != 0
    {
        return Err("private ACK carrier substitutes its identifier, envelope or signed object".into());
    }
    Ok((tx, lease))
}

/// One-input, one-change, no-replacement construction path used above. Each
/// allocation family follows the production builder, including simultaneously
/// live priced/unsigned payloads and ScriptBuilder's geometric byte buffers.
pub(super) fn reserve_carrier_build_v4(
    object: &PalwConsensusObjectV2,
    funding: &kaspa_consensus_core::tx::UtxoEntry,
    public_key_bytes: usize,
) -> Result<Option<NativeDemandLeaseV1>, String> {
    use kaspa_consensus_core::tx::{TransactionInput, TransactionOutput, UtxoEntry};
    let payload = borsh::object_length(&(PALW_LIFECYCLE_TX_VERSION_V2, object)).map_err(|e| e.to_string())?;
    let signature = kaspa_txscript::MLDSA87_SIG_LEN.checked_add(1).ok_or("carrier signature length overflow")?;
    let script = signature.checked_add(public_key_bytes).and_then(|n| n.checked_add(10)).ok_or("carrier script overflow")?;
    reserve_carrier_parts_v4(&[
        (std::mem::size_of::<Transaction>(), 3),
        (std::mem::size_of::<TransactionInput>(), 2),
        // Vec::new().push(change) has the allocator's four-element minimum.
        (std::mem::size_of::<TransactionOutput>(), 8),
        (std::mem::size_of::<UtxoEntry>(), 1),
        (std::mem::size_of::<Option<UtxoEntry>>(), 1),
        (std::mem::size_of::<kaspa_consensus_core::tx::MutableTransaction<Transaction>>(), 1),
        (payload, 3),
        (signature, 3),
        (script, 4),
        (kaspa_txscript::script_builder::DEFAULT_SCRIPT_ALLOC, 2),
        (funding.script_public_key.script().len(), 6),
    ])
}

fn reserve_carrier_parts_v4(parts: &[(usize, u64)]) -> Result<Option<NativeDemandLeaseV1>, String> {
    let bytes = parts
        .iter()
        .try_fold(0u64, |total, &(size, count)| {
            u64::try_from(size).ok().and_then(|size| size.checked_mul(count)).and_then(|size| total.checked_add(size))
        })
        .ok_or("carrier typed allocation request overflow")?;
    reserve_native_requests_v1(bytes).map_err(|e| e.to_string())
}
