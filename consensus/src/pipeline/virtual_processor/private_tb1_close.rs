//! Branch-owned EXEC close admission. Private publications without a native
//! preparation remain readable by the legacy off-chain observer.
use super::{VirtualStateProcessor, processor::PalwLocalTransitionFaultV1};
use crate::{
    errors::RuleError,
    model::{
        services::reachability::ReachabilityService,
        stores::{block_transactions::BlockTransactionsStoreReader, ghostdag::GhostdagStoreReader, headers::HeaderStoreReader},
    },
};
use kaspa_consensus_core::{
    Hash64,
    api::ConsensusApi,
    block::Block,
    coinbase::{coinbase_payload_extra_data, palw_coinbase_marker},
    errors::consensus::{ConsensusError, ConsensusResult},
    header::Header,
    palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable,
    palw_state_v2::{PalwBlockContextV2, PalwChainStateV2},
    palw_tb1::{
        exec_close_carrier::ExecCloseCarrier,
        moe2_exec_close::{VerifiedMoe2ExecClose, verify_exec_close_with_rules},
        publication::{ExecCertificateReceipt, ExecCertificateReference, PublicationLimits, verify_prepared_exec_certificate},
        qualification::QualifiedExecPanel,
    },
    trusted::ExternalGhostdagData,
};
use std::sync::Arc;

struct CloseHistory<'a>(&'a VirtualStateProcessor);

impl ConsensusApi for CloseHistory<'_> {
    fn get_block(&self, block: Hash64) -> ConsensusResult<Block> {
        let header = self.get_header(block)?;
        let txs = self.0.block_transactions_store.get(block).map_err(|_| ConsensusError::BlockNotFound(block))?;
        Ok(Block::from_arcs(header, txs))
    }

    fn get_header(&self, block: Hash64) -> ConsensusResult<Arc<Header>> {
        self.0.headers_store.get_header(block).map_err(|_| ConsensusError::HeaderNotFound(block))
    }

    fn get_ghostdag_data(&self, block: Hash64) -> ConsensusResult<ExternalGhostdagData> {
        self.0.ghostdag_store.get_data(block).map(|data| (&*data).into()).map_err(|_| ConsensusError::MissingData(block))
    }

    fn is_chain_ancestor_of(&self, low: Hash64, high: Hash64) -> ConsensusResult<bool> {
        self.0.reachability_service.try_is_chain_ancestor_of(low, high).map_err(|_| ConsensusError::MissingData(high))
    }

    fn palw_coinbase_commits_to(&self, chain: [u8; 64], block: Hash64, expected: &[u8; 64]) -> ConsensusResult<bool> {
        if chain != self.0.palw_network_domain_v2().as_bytes() {
            return Ok(false);
        }
        let txs = self.0.block_transactions_store.get(block).map_err(|_| ConsensusError::BlockNotFound(block))?;
        Ok(txs.first().and_then(|tx| coinbase_payload_extra_data(&tx.payload)).and_then(palw_coinbase_marker).as_ref()
            == Some(expected))
    }
}

impl VirtualStateProcessor {
    pub(super) fn resolve_private_exec_certificate(
        &self,
        candidate: &Block,
        parent: Hash64,
        state: &PalwChainStateV2,
    ) -> Result<Option<ExecCertificateReceipt>, PalwLocalTransitionFaultV1> {
        let Some(rules) = self.private_moe2_exec_rules.filter(|rules| candidate.header.daa_score >= rules.private_from) else {
            return Ok(None);
        };
        let Ok(Some(reference)) = ExecCertificateReference::from_block(candidate) else {
            return Ok(None);
        };
        let Some(close) = state.tb1_exec_close_receipt(reference.close) else { return Ok(None) };
        if state.tb1_exec_certificate(reference.close).is_some() {
            return Ok(None);
        }
        let Some((exec, _)) = state.tb1_exec_panel_preparation(close.preparation, parent) else { return Ok(None) };
        let round = exec.round();
        if !(round.release_from_daa..round.deadline_exclusive_daa).contains(&candidate.header.daa_score) {
            return Ok(None);
        }
        let close_limit = rules.max_block_mass as usize;
        let Ok(limits) = PublicationLimits::for_native_batch(close_limit, &exec.qualification()) else { return Ok(None) };
        let missing = || PalwLocalTransitionFaultV1::Context(HistoryUnavailable);
        // Resolve the batch from the selected history. An arbitrary lookup
        // hash supplied by a candidate cannot make that history unavailable.
        let mut batch = parent;
        let observation_point = loop {
            self.check_shutdown_boundary()?;
            let header = self.headers_store.get_header(batch).map_err(|_| missing())?;
            if header.daa_score < round.release_from_daa || batch == reference.close {
                return Ok(None);
            }
            let predecessor = self.ghostdag_store.get_selected_parent(batch).map_err(|_| missing())?;
            if batch == reference.batch {
                break predecessor;
            }
            batch = predecessor;
        };
        for point in [observation_point, parent] {
            let pruning = self.headers_store.get_header(point).map_err(|_| missing())?.pruning_point;
            if !self.reachability_service.try_is_chain_ancestor_of(reference.close, pruning).map_err(|_| missing())? {
                return Ok(None);
            }
        }
        let branch_state = |anchor| self.restore_private_moe2_state_at(state, anchor).map_err(private_tb1_history_fault);
        let Some(observation) = self.native_selected_branch_settlement(branch_state(observation_point)?)? else {
            return Ok(None);
        };
        let Some(settlement) = self.native_selected_branch_settlement(branch_state(parent)?)? else {
            return Ok(None);
        };
        match verify_prepared_exec_certificate(
            &CloseHistory(self),
            parent,
            state,
            reference.close,
            close_limit,
            &observation,
            &settlement,
            batch,
            candidate,
            limits,
        ) {
            Ok(receipt) => Ok(Some(receipt)),
            Err(error) if matches!(error.kind(), std::io::ErrorKind::InvalidData | std::io::ErrorKind::TimedOut) => Ok(None),
            Err(_) => Err(missing()),
        }
    }

    pub(super) fn resolve_private_exec_close(
        &self,
        point: &PalwBlockContextV2,
        header: &Arc<Header>,
        selected_parent: Hash64,
        state: &PalwChainStateV2,
    ) -> Result<Option<VerifiedMoe2ExecClose>, PalwLocalTransitionFaultV1> {
        let Some(rules) = self.private_moe2_exec_rules.filter(|rules| point.daa_score >= rules.private_from) else {
            return Ok(None);
        };
        let missing = || PalwLocalTransitionFaultV1::Context(HistoryUnavailable);
        let txs = self.block_transactions_store.get(header.hash).map_err(|_| missing())?;
        let candidate = Block::from_arcs(header.clone(), txs);
        let Ok(preparation) = ExecCloseCarrier::preparation_from_block(&candidate, rules.max_block_mass as usize) else {
            return Ok(None);
        };
        let Some((exec, panel)) = state.tb1_exec_panel_preparation(preparation, selected_parent) else {
            return Ok(None);
        };
        let Ok(rounds) = QualifiedExecPanel::new(exec.round(), &exec.qualification(), panel.round(), &panel.qualification()) else {
            return Ok(None);
        };
        let history = CloseHistory(self);
        if !history.is_chain_ancestor_of(preparation, selected_parent).map_err(|_| missing())? {
            return Ok(None);
        }
        let quote_parent = history.get_ghostdag_data(preparation).map_err(|_| missing())?.selected_parent;
        let price_state = self.restore_private_moe2_state_at(state, quote_parent).map_err(private_tb1_history_fault)?;
        let marker =
            candidate.transactions.first().and_then(|tx| coinbase_payload_extra_data(&tx.payload)).and_then(palw_coinbase_marker);
        match verify_exec_close_with_rules(
            &history,
            rules,
            point,
            &candidate,
            selected_parent,
            |_, expected| Ok(marker.as_ref() == Some(expected)),
            &rounds,
            state,
            &price_state,
            usize::from(u16::MAX),
            usize::MAX,
        ) {
            Ok(close) => Ok(Some(close)),
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => Ok(None),
            Err(_) => Err(PalwLocalTransitionFaultV1::Context(HistoryUnavailable)),
        }
    }
}

fn private_tb1_history_fault(error: RuleError) -> PalwLocalTransitionFaultV1 {
    match error {
        RuleError::ConsensusExiting => PalwLocalTransitionFaultV1::Shutdown,
        RuleError::LocalNativeCapacityUnavailable(why) => why.into(),
        RuleError::LocalNativeStateContextUnavailable(why) => PalwLocalTransitionFaultV1::Context(why),
        _ => PalwLocalTransitionFaultV1::Context(HistoryUnavailable),
    }
}
