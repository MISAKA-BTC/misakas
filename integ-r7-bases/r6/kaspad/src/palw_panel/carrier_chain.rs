//! Node-local carrier funding and unconfirmed-count ownership. The existing
//! chain/mempool reads choose recovery, back-pressure and replacement policy.
//! A submitted transaction or its change is never acceptance of a PALW object.
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint, TransactionOutput, UtxoEntry};

pub(super) type CarrierFundingV1 = Option<(TransactionOutpoint, UtxoEntry)>;

#[derive(Default)]
pub(super) struct CarrierChainV1 {
    funding: CarrierFundingV1,
    inflight: usize,
}
impl CarrierChainV1 {
    pub(super) fn funding(&self) -> &CarrierFundingV1 {
        &self.funding
    }
    pub(super) fn set_funding(&mut self, funding: CarrierFundingV1) {
        self.funding = funding;
    }
    pub(super) fn inflight(&self) -> usize {
        self.inflight
    }
    pub(super) fn set_inflight(&mut self, inflight: usize) {
        self.inflight = inflight;
    }
    pub(super) fn book(&mut self) -> CarrierBookMutV1<'_> {
        CarrierBookMutV1::new(&mut self.funding, &mut self.inflight)
    }
    /// A held tick has no spendable funding; its persistent tip still exists.
    /// Borrow the chosen tick funding and the same persistent counter without
    /// implicitly writing that funding back or changing recovery policy.
    pub(super) fn book_with_funding<'a>(&'a mut self, funding: &'a mut CarrierFundingV1) -> CarrierBookMutV1<'a> {
        CarrierBookMutV1::new(funding, &mut self.inflight)
    }
}

/// A single mutable capability for a send site, including a lane's existing state
/// or a tick's spendable funding. Neither field is independently exposed to senders.
pub(super) struct CarrierBookMutV1<'a> {
    funding: &'a mut CarrierFundingV1,
    inflight: &'a mut usize,
}

#[derive(Clone, Copy)]
pub(super) enum CarrierInFlightUpdateV1 {
    Increment,
    SaturatingIncrement,
    /// Existing replacement sites set their old in-flight count explicitly.
    Set(usize),
}
impl<'a> CarrierBookMutV1<'a> {
    pub(super) fn new(funding: &'a mut CarrierFundingV1, inflight: &'a mut usize) -> Self {
        Self { funding, inflight }
    }
    pub(super) fn funding(&self) -> &CarrierFundingV1 {
        self.funding
    }
    pub(super) fn set_funding(&mut self, funding: CarrierFundingV1) {
        *self.funding = funding;
    }
    pub(super) fn inflight(&self) -> usize {
        *self.inflight
    }
    /// Apply the already chosen submission's change at the caller's old success
    /// bookkeeping point. Keep persist -> funding -> count, including the old
    /// warn-and-continue persistence callback and ordinary checked increment.
    fn record_change(
        &mut self,
        txid: TransactionId,
        change_index: u32,
        change: TransactionOutput,
        submitted_daa: u64,
        count: CarrierInFlightUpdateV1,
        persist: impl FnOnce(TransactionOutpoint),
    ) -> TransactionOutpoint {
        let next = TransactionOutpoint::new(txid, change_index);
        persist(next);
        *self.funding = Some((
            next,
            UtxoEntry {
                amount: change.value,
                script_public_key: change.script_public_key,
                block_daa_score: submitted_daa,
                is_coinbase: false,
            },
        ));
        match count {
            CarrierInFlightUpdateV1::Increment => *self.inflight += 1,
            CarrierInFlightUpdateV1::SaturatingIncrement => *self.inflight = self.inflight.saturating_add(1),
            CarrierInFlightUpdateV1::Set(inflight) => *self.inflight = inflight,
        }
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::tx::ScriptPublicKey;

    fn change() -> TransactionOutput {
        TransactionOutput::new(100, ScriptPublicKey::from_vec(0, vec![0x51]))
    }
    fn initial() -> CarrierFundingV1 {
        Some((
            TransactionOutpoint::new(TransactionId::from_u64_word(1), 0),
            UtxoEntry { amount: 200, script_public_key: change().script_public_key, block_daa_score: 7, is_coinbase: false },
        ))
    }

    #[test]
    fn held_tick_and_refused_spend_do_not_erase_the_persistent_tip_or_free_its_slot() {
        let mut chain = CarrierChainV1::default();
        chain.set_funding(initial());
        chain.set_inflight(1);
        let mut tick_funding = None;
        {
            let mut tick = chain.book_with_funding(&mut tick_funding);
            assert!(tick.funding().is_none());
            tick.set_funding(None);
            assert_eq!(tick.inflight(), 1, "a refusal does not infer confirmation or free the slot");
        }
        assert_eq!(chain.funding(), &initial(), "a held tick has not committed its spendable funding");
        // At the existing replacement success point, only the tick's funding is advanced.
        let txid = TransactionId::from_u64_word(2);
        let mut persisted = Vec::new();
        chain
            .book_with_funding(&mut tick_funding)
            .record_change(txid, 0, change(), 8, CarrierInFlightUpdateV1::Set(1), |next| persisted.push(next));
        assert_eq!(persisted, vec![TransactionOutpoint::new(txid, 0)]);
        assert_eq!(chain.funding(), &initial());
        assert_eq!(chain.inflight(), 1);
        chain.set_funding(tick_funding); // The old `!held` write-back point, explicitly chosen by the caller.
        assert_eq!(chain.funding().as_ref().unwrap().0, persisted[0]);
        assert_eq!(chain.funding().as_ref().unwrap().1.block_daa_score, 8);
    }

    #[test]
    fn a_persistence_callback_panic_precedes_memory_updates() {
        let mut chain = CarrierChainV1::default();
        chain.set_funding(initial());
        chain.set_inflight(1);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            chain.book().record_change(TransactionId::from_u64_word(2), 1, change(), 8, CarrierInFlightUpdateV1::Increment, |_| {
                panic!("injected persistence callback panic")
            });
        }));
        assert!(result.is_err());
        assert_eq!(chain.funding(), &initial());
        assert_eq!(chain.inflight(), 1);
        // The real callback warns and returns on an I/O error: returning still advances exactly as before.
        chain.book().record_change(TransactionId::from_u64_word(2), 1, change(), 8, CarrierInFlightUpdateV1::Increment, |_| {});
        assert_eq!(chain.funding().as_ref().unwrap().0.index, 1);
        assert_eq!(chain.inflight(), 2);
    }
}

/// Built transaction plus the exact change snapshot the old caller held across submission.
/// Its existence says nothing about submission or native acceptance.
pub(super) struct PreparedCarrierV1 {
    tx: kaspa_consensus_core::tx::Transaction,
    change: CarrierChangeV1,
}
struct CarrierChangeV1 {
    txid: TransactionId,
    index: u32,
    output: TransactionOutput,
    daa: u64,
}
impl PreparedCarrierV1 {
    pub(super) fn new(tx: kaspa_consensus_core::tx::Transaction, index: u32, daa: u64) -> Self {
        let txid = tx.id();
        let output = tx.outputs[index as usize].clone();
        Self { tx, change: CarrierChangeV1 { txid, index, output, daa } }
    }
    pub(super) fn id(&self) -> TransactionId {
        self.change.txid
    }
}
/// Only the submission methods below construct this token. It means the existing
/// node submission door returned success, never that the chain accepted a PALW object.
pub(super) struct SubmittedCarrierV1 {
    change: CarrierChangeV1,
}
impl SubmittedCarrierV1 {
    pub(super) fn record_change(
        self,
        book: &mut CarrierBookMutV1<'_>,
        count: CarrierInFlightUpdateV1,
        persist: impl FnOnce(TransactionOutpoint),
    ) -> TransactionOutpoint {
        let CarrierChangeV1 { txid, index, output, daa } = self.change;
        book.record_change(txid, index, output, daa, count, persist)
    }
}
impl super::PalwPanelService {
    /// Existing lifecycle builder and its resource/error contract, unchanged.
    pub(super) fn prepare_lifecycle_carrier_v1(
        &self,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
        outpoint: TransactionOutpoint,
        entry: &UtxoEntry,
        daa: u64,
    ) -> Result<PreparedCarrierV1, String> {
        self.build_lifecycle_tx(object, outpoint, entry).map(|tx| PreparedCarrierV1::new(tx, 0, daa))
    }
    /// Retain the existing drill filter and stuck-tip replacement door.
    pub(super) async fn submit_prepared_carrier_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        prepared: PreparedCarrierV1,
    ) -> Result<SubmittedCarrierV1, String> {
        let PreparedCarrierV1 { tx, change } = prepared;
        self.submit_carrier_v1(session, tx).await?;
        Ok(SubmittedCarrierV1 { change })
    }
    /// Canonical/bootstrap/evaluation callers keep their original direct RPC door
    /// and exact ProtocolError; they do not acquire the overlay drill/RBF policy.
    pub(super) async fn submit_prepared_direct_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        prepared: PreparedCarrierV1,
    ) -> Result<SubmittedCarrierV1, kaspa_p2p_lib::common::ProtocolError> {
        let PreparedCarrierV1 { tx, change } = prepared;
        self.flow_context.submit_rpc_transaction(session, tx, kaspa_mining::mempool::tx::Orphan::Forbidden).await?;
        Ok(SubmittedCarrierV1 { change })
    }
    /// Mandatory RBF keeps its original error and removed-transaction value.
    pub(super) async fn submit_prepared_replacement_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        prepared: PreparedCarrierV1,
    ) -> Result<(SubmittedCarrierV1, std::sync::Arc<kaspa_consensus_core::tx::Transaction>), kaspa_p2p_lib::common::ProtocolError>
    {
        let PreparedCarrierV1 { tx, change } = prepared;
        let replaced = self.flow_context.submit_rpc_transaction_replacement(session, tx).await?;
        Ok((SubmittedCarrierV1 { change }, replaced))
    }
}
