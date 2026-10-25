use super::*;
use kaspa_consensus_core::{
    BlockHash,
    api::ConsensusApi,
    config::params::MAINNET_PARAMS,
    errors::consensus::{ConsensusError, ConsensusResult},
    evm::{EvmStateSnapshot, MISAKA_MLDSA_VERIFY_PRECOMPILE},
    header::Header,
};
use std::sync::Mutex;

struct HeaderReader {
    result: ConsensusResult<Arc<Header>>,
    reads: Mutex<Vec<BlockHash>>,
}
impl HeaderReader {
    fn at(daa: u64) -> Self {
        let mut header: Header = (&MAINNET_PARAMS.genesis).into();
        header.daa_score = daa;
        Self { result: Ok(Arc::new(header)), reads: Mutex::new(vec![]) }
    }
}
impl ConsensusApi for HeaderReader {
    fn get_header(&self, hash: BlockHash) -> ConsensusResult<Arc<Header>> {
        self.reads.lock().unwrap().push(hash);
        self.result.clone()
    }
}
fn target(n: u8) -> BlockHash {
    BlockHash::from_bytes([n; 64])
}

#[test]
fn activation_daa_read_error_stops_env_and_simulation_continuation() {
    for exact in [target(2), target(3)] {
        // canonical-head and historical resolved identities
        for fault in [ConsensusError::HeaderNotFound(exact), ConsensusError::General("header read failed")] {
            let reader = HeaderReader { result: Err(fault), reads: Mutex::new(vec![]) };
            let called = std::cell::Cell::new(0);
            let result = with_executed_l1_daa(&reader, exact, |_| {
                called.set(called.get() + 1);
                Ok(())
            });
            assert_eq!(result.unwrap_err().code, -32000);
            assert_eq!(called.get(), 0, "no env/simulation on unavailable activation metadata");
            assert_eq!(*reader.reads.lock().unwrap(), vec![exact]);
        }
    }
}

#[test]
fn activation_daa_exact_header_preserves_true_zero_and_fence_boundaries() {
    for (daa, fence, active) in [
        (0, 0, true),
        (0, 1, false),
        (9, 10, false),
        (10, 10, true),
        (11, 10, true),
        (11, u64::MAX, false),
        (u64::MAX, u64::MAX, true),
    ] {
        let reader = HeaderReader::at(daa);
        let actual = with_executed_l1_daa(&reader, target(7), |read| {
            assert_eq!(read, daa);
            Ok(read >= fence)
        })
        .unwrap();
        assert_eq!(actual, active);
        assert_eq!(*reader.reads.lock().unwrap(), vec![target(7)]);
    }
}

#[test]
fn activation_daa_real_simulation_handler_follows_metadata_not_failure_zero() {
    for (daa, fence, active) in [(0, 0, true), (0, u64::MAX, false), (9, 10, false), (10, 10, true), (11, 10, true)] {
        let reader = HeaderReader::at(daa);
        let result = with_executed_l1_daa(&reader, target(8), |read| {
            let env = kaspa_evm::sim::EthCallEnv {
                chain_id: EVM_CHAIN_ID,
                gas_limit: 30_000_000,
                f003_active: read >= fence,
                ..Default::default()
            };
            // F003 rejects malformed calldata with ABI false; inert empty account returns empty bytes.
            let call = kaspa_evm::sim::EthCall { to: Some(MISAKA_MLDSA_VERIFY_PRECOMPILE), data: vec![], ..Default::default() };
            kaspa_evm::sim::simulate_call(&EvmStateSnapshot::default(), &env, &call)
                .map_err(|e| EthRpcError::server(format!("simulation: {e}")))
        })
        .unwrap();
        assert!(result.success);
        assert_eq!(result.output, if active { vec![0; 32] } else { vec![] });
        assert_eq!(*reader.reads.lock().unwrap(), vec![target(8)]);
    }
}
