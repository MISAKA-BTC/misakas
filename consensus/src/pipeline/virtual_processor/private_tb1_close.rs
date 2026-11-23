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
        qualification::QualifiedExecPanel,
    },
    trusted::ExternalGhostdagData,
};
use std::sync::Arc;

struct CloseHistory<'a>(&'a VirtualStateProcessor);

impl ConsensusApi for CloseHistory<'_> {
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
        let price_state = self.restore_private_moe2_state_at(state, quote_parent).map_err(|error| match error {
            RuleError::ConsensusExiting => PalwLocalTransitionFaultV1::Shutdown,
            RuleError::LocalNativeCapacityUnavailable(why) => why.into(),
            RuleError::LocalNativeStateContextUnavailable(why) => PalwLocalTransitionFaultV1::Context(why),
            _ => PalwLocalTransitionFaultV1::Context(HistoryUnavailable),
        })?;
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
