#[cfg(test)]
mod mockery {

    use crate::model::*;
    use kaspa_addresses::{Prefix, Version};
    use kaspa_consensus_core::api::BlockCount;
    use kaspa_consensus_core::network::NetworkType;
    use kaspa_consensus_core::subnets::SubnetworkId;
    use kaspa_consensus_core::tx::ScriptPublicKey;
    use kaspa_hashes::Hash;
    use kaspa_math::Uint192;
    use kaspa_notify::subscription::Command;
    use kaspa_rpc_macros::test_wrpc_serializer as test;
    use kaspa_txscript::script_class::ScriptClass;
    use kaspa_utils::networking::{ContextualNetAddress, IpAddress, NetAddress};
    use rand::Rng;
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::Arc;
    use uuid::Uuid;
    use workflow_serializer::prelude::*;

    // this trait is used to generate random
    // values for testing on various data types
    trait Mock {
        fn mock() -> Self;
    }

    impl<T> Mock for Option<T>
    where
        T: Mock,
    {
        fn mock() -> Self {
            Some(T::mock())
        }
    }

    impl<T> Mock for Vec<T>
    where
        T: Mock,
    {
        fn mock() -> Self {
            vec![T::mock()]
        }
    }

    impl<T> Mock for Arc<T>
    where
        T: Mock,
    {
        fn mock() -> Self {
            Arc::new(T::mock())
        }
    }

    fn mock<T>() -> T
    where
        T: Mock,
    {
        // forward to the type's Mock implementation
        T::mock()
    }

    // this function tests serialization and deserialization of a type
    // by serializing it (A), deserializing it, serializing it again (B)
    // and comparing A and B buffers.
    fn test<T>(kind: &str)
    where
        T: Serializer + Deserializer + Mock,
    {
        let data = T::mock();

        const PREFIX: u32 = 0x12345678;
        const SUFFIX: u32 = 0x90abcdef;

        let mut buffer1 = Vec::new();
        let writer = &mut buffer1;
        store!(u32, &PREFIX, writer).unwrap();
        serialize!(T, &data, writer).unwrap();
        store!(u32, &SUFFIX, writer).unwrap();

        let reader = &mut buffer1.as_slice();
        let prefix: u32 = load!(u32, reader).unwrap();
        // this will never occur, but it's a good practice to check in case
        // the serialization/deserialization logic changes in the future
        assert_eq!(prefix, PREFIX, "misalignment when consuming serialized buffer in `{kind}`");
        let tmp = deserialize!(T, reader).unwrap();
        let suffix: u32 = load!(u32, reader).unwrap();
        assert_eq!(suffix, SUFFIX, "misalignment when consuming serialized buffer in `{kind}`");

        let mut buffer2 = Vec::new();
        let writer = &mut buffer2;
        store!(u32, &PREFIX, writer).unwrap();
        serialize!(T, &tmp, writer).unwrap();
        store!(u32, &SUFFIX, writer).unwrap();

        assert!(buffer1 == buffer2, "serialization/deserialization failure while testing `{kind}`");
    }

    #[macro_export]
    macro_rules! impl_mock {
        ($($type:ty),*) => {
            $(impl Mock for $type {
                fn mock() -> Self {
                    rand::thread_rng().r#gen()
                }
            })*
        };
    }

    impl_mock!(bool, u8, u16, u32, f32, u64, i64, f64);

    impl Mock for Uint192 {
        fn mock() -> Self {
            Uint192([mock(), mock(), mock()])
        }
    }

    // kaspa-pq PR-8.5: BlueWorkType widened from Uint192 to Uint576.
    impl Mock for kaspa_math::Uint576 {
        fn mock() -> Self {
            kaspa_math::Uint576([mock(), mock(), mock(), mock(), mock(), mock(), mock(), mock(), mock()])
        }
    }

    impl Mock for SubnetworkId {
        fn mock() -> Self {
            let mut bytes: [u8; 20] = [0; 20];
            rand::thread_rng().fill(&mut bytes);
            SubnetworkId::from_bytes(bytes)
        }
    }

    impl Mock for Hash {
        fn mock() -> Self {
            let mut bytes: [u8; 32] = [0; 32];
            rand::thread_rng().fill(&mut bytes);
            Hash::from_bytes(bytes)
        }
    }

    // PR-9.5c/f: the consensus-identity aliases (TransactionId,
    // TransactionHash, MerkleRoot, AcceptedIdMerkleRoot) are now
    // `Hash64`; mock random 64-byte values for them. `rand`'s
    // `fill` works on `[u8; 64]` (it operates on slices), unlike
    // the `Standard` distribution which caps at 32.
    impl Mock for kaspa_consensus_core::Hash64 {
        fn mock() -> Self {
            let mut bytes: [u8; 64] = [0; 64];
            rand::thread_rng().fill(&mut bytes[..]);
            kaspa_consensus_core::Hash64::from_bytes(bytes)
        }
    }

    impl Mock for RpcAddress {
        fn mock() -> Self {
            RpcAddress::new(Prefix::Mainnet, Version::PubKey, Hash::mock().as_bytes().as_slice())
        }
    }

    impl Mock for RpcHeader {
        fn mock() -> Self {
            RpcHeader {
                version: mock(),
                timestamp: mock(),
                bits: mock(),
                nonce: mock(),
                hash_merkle_root: mock(),
                accepted_id_merkle_root: mock(),
                utxo_commitment: mock(),
                hash: mock(),
                parents_by_level: vec![mock()],
                daa_score: mock(),
                blue_score: mock(),
                blue_work: mock(),
                pruning_point: mock(),
                // kaspa-pq: ADR-0007 algo id + ADR-0020 EVM commitments (wire v3).
                pow_algo_id: mock(),
                evm_payload_hash: mock(),
                evm_commitment_root: mock(),
                overlay_commitment_root: mock(),
                palw_state_root: Default::default(),
                // MISAKA ADR-0038 (wire v5): the post-PoW PALW commitment.
                palw_commitment: vec![0xAB; 32],
            }
        }
    }

    impl Mock for RpcRawHeader {
        fn mock() -> Self {
            RpcRawHeader {
                version: mock(),
                timestamp: mock(),
                bits: mock(),
                nonce: mock(),
                hash_merkle_root: mock(),
                accepted_id_merkle_root: mock(),
                utxo_commitment: mock(),
                parents_by_level: vec![mock()],
                daa_score: mock(),
                blue_score: mock(),
                blue_work: mock(),
                pruning_point: mock(),
                // kaspa-pq: ADR-0007 algo id + ADR-0020 EVM commitments (wire v3).
                pow_algo_id: mock(),
                evm_payload_hash: mock(),
                evm_commitment_root: mock(),
                overlay_commitment_root: mock(),
                palw_state_root: Default::default(),
                // MISAKA ADR-0038 (wire v5): the post-PoW PALW commitment.
                palw_commitment: vec![0xAB; 32],
            }
        }
    }

    impl Mock for RpcBlockVerboseData {
        fn mock() -> Self {
            RpcBlockVerboseData {
                hash: mock(),
                difficulty: mock(),
                selected_parent_hash: mock(),
                transaction_ids: mock(),
                is_header_only: mock(),
                blue_score: mock(),
                children_hashes: mock(),
                merge_set_blues_hashes: mock(),
                merge_set_reds_hashes: mock(),
                is_chain_block: mock(),
            }
        }
    }

    impl Mock for RpcBlock {
        fn mock() -> Self {
            // kaspa-pq EVM Lane v0.4 (wire v2): non-empty payload bytes exercise the
            // new field through the serializer round-trip.
            RpcBlock { header: mock(), transactions: mock(), verbose_data: mock(), evm_payload: mock() }
        }
    }

    impl Mock for RpcRawBlock {
        fn mock() -> Self {
            RpcRawBlock { header: mock(), transactions: mock(), evm_payload: mock() }
        }
    }

    impl Mock for RpcOptionalTransactionInputVerboseData {
        fn mock() -> Self {
            RpcOptionalTransactionInputVerboseData { utxo_entry: mock() }
        }
    }

    impl Mock for RpcTransactionInput {
        fn mock() -> Self {
            RpcTransactionInput {
                previous_outpoint: mock(),
                signature_script: Hash::mock().as_bytes().to_vec(),
                sequence: mock(),
                sig_op_count: mock(),
                verbose_data: mock(),
            }
        }
    }

    impl Mock for RpcOptionalTransactionInput {
        fn mock() -> Self {
            RpcOptionalTransactionInput {
                previous_outpoint: mock(),
                signature_script: Some(Hash::mock().as_bytes().to_vec()),
                sequence: mock(),
                sig_op_count: mock(),
                verbose_data: mock(),
            }
        }
    }

    impl Mock for RpcOptionalTransactionOutpoint {
        fn mock() -> Self {
            RpcOptionalTransactionOutpoint { transaction_id: mock(), index: mock() }
        }
    }

    impl Mock for RpcTransactionOutputVerboseData {
        fn mock() -> Self {
            RpcTransactionOutputVerboseData { script_public_key_type: mock(), script_public_key_address: mock() }
        }
    }

    impl Mock for RpcOptionalTransactionOutputVerboseData {
        fn mock() -> Self {
            RpcOptionalTransactionOutputVerboseData { script_public_key_type: mock(), script_public_key_address: mock() }
        }
    }

    impl Mock for RpcTransactionOutput {
        fn mock() -> Self {
            RpcTransactionOutput { value: mock(), script_public_key: mock(), verbose_data: mock() }
        }
    }

    impl Mock for RpcOptionalTransactionOutput {
        fn mock() -> Self {
            RpcOptionalTransactionOutput { value: mock(), script_public_key: mock(), verbose_data: mock() }
        }
    }

    impl Mock for RpcTransactionVerboseData {
        fn mock() -> Self {
            RpcTransactionVerboseData {
                transaction_id: mock(),
                hash: mock(),
                compute_mass: mock(),
                block_hash: mock(),
                block_time: mock(),
            }
        }
    }

    impl Mock for RpcOptionalTransactionVerboseData {
        fn mock() -> Self {
            RpcOptionalTransactionVerboseData {
                transaction_id: mock(),
                hash: mock(),
                compute_mass: mock(),
                block_hash: mock(),
                block_time: mock(),
            }
        }
    }

    impl Mock for RpcUtxoEntryVerbosity {
        fn mock() -> Self {
            RpcUtxoEntryVerbosity {
                include_amount: mock(),
                include_script_public_key: mock(),
                include_block_daa_score: mock(),
                include_is_coinbase: mock(),
                verbose_data_verbosity: mock(),
            }
        }
    }

    impl Mock for RpcUtxoEntryVerboseDataVerbosity {
        fn mock() -> Self {
            RpcUtxoEntryVerboseDataVerbosity { include_script_public_key_type: mock(), include_script_public_key_address: mock() }
        }
    }

    impl Mock for RpcTransactionInputVerboseDataVerbosity {
        fn mock() -> Self {
            RpcTransactionInputVerboseDataVerbosity { utxo_entry_verbosity: mock() }
        }
    }

    impl Mock for RpcTransactionInputVerboseData {
        fn mock() -> Self {
            RpcTransactionInputVerboseData {}
        }
    }

    impl Mock for RpcTransactionInputVerbosity {
        fn mock() -> Self {
            RpcTransactionInputVerbosity {
                include_previous_outpoint: mock(),
                include_signature_script: mock(),
                include_sequence: mock(),
                include_sig_op_count: mock(),
                verbose_data_verbosity: mock(),
            }
        }
    }

    impl Mock for RpcTransactionOutputVerbosity {
        fn mock() -> Self {
            RpcTransactionOutputVerbosity { include_amount: mock(), include_script_public_key: mock(), verbose_data_verbosity: mock() }
        }
    }

    impl Mock for RpcTransactionOutputVerboseDataVerbosity {
        fn mock() -> Self {
            RpcTransactionOutputVerboseDataVerbosity {
                include_script_public_key_type: mock(),
                include_script_public_key_address: mock(),
            }
        }
    }

    impl Mock for RpcTransactionVerboseDataVerbosity {
        fn mock() -> Self {
            RpcTransactionVerboseDataVerbosity {
                include_transaction_id: mock(),
                include_hash: mock(),
                include_compute_mass: mock(),
                include_block_hash: mock(),
                include_block_time: mock(),
            }
        }
    }

    impl Mock for RpcTransactionVerbosity {
        fn mock() -> Self {
            RpcTransactionVerbosity {
                include_version: mock(),
                input_verbosity: mock(),
                output_verbosity: mock(),
                include_lock_time: mock(),
                include_subnetwork_id: mock(),
                include_gas: mock(),
                include_payload: mock(),
                include_mass: mock(),
                verbose_data_verbosity: mock(),
            }
        }
    }

    impl Mock for RpcTransaction {
        fn mock() -> Self {
            RpcTransaction {
                version: mock(),
                inputs: mock(),
                outputs: mock(),
                lock_time: mock(),
                subnetwork_id: mock(),
                gas: mock(),
                payload: Hash::mock().as_bytes().to_vec(),
                mass: mock(),
                verbose_data: mock(),
            }
        }
    }

    impl Mock for RpcOptionalTransaction {
        fn mock() -> Self {
            RpcOptionalTransaction {
                version: mock(),
                inputs: mock(),
                outputs: mock(),
                lock_time: mock(),
                subnetwork_id: mock(),
                gas: mock(),
                payload: Some(Hash::mock().as_bytes().to_vec()),
                mass: mock(),
                verbose_data: mock(),
            }
        }
    }

    impl Mock for RpcOptionalHeader {
        fn mock() -> Self {
            RpcOptionalHeader {
                version: mock(),
                timestamp: mock(),
                bits: mock(),
                nonce: mock(),
                hash_merkle_root: mock(),
                accepted_id_merkle_root: mock(),
                utxo_commitment: mock(),
                hash: mock(),
                parents_by_level: mock(),
                daa_score: mock(),
                blue_score: mock(),
                blue_work: mock(),
                pruning_point: mock(),
            }
        }
    }

    impl Mock for RpcCompressedParents {
        fn mock() -> Self {
            // PR-9.5e: parents are block hashes (Hash64); CompressedParents is built from BlockHash.
            let empty: Vec<(u8, Vec<kaspa_consensus_core::BlockHash>)> = vec![];
            empty.try_into().expect("It should not fail.")
        }
    }

    impl Mock for RpcNodeId {
        fn mock() -> Self {
            RpcNodeId::new(Uuid::new_v4())
        }
    }

    impl Mock for IpAddr {
        fn mock() -> Self {
            IpAddr::V4(Ipv4Addr::new(mock(), mock(), mock(), mock()))
        }
    }

    impl Mock for IpAddress {
        fn mock() -> Self {
            IpAddress::new(mock())
        }
    }

    impl Mock for NetAddress {
        fn mock() -> Self {
            NetAddress::new(IpAddress::new(mock()), mock())
        }
    }

    impl Mock for ContextualNetAddress {
        fn mock() -> Self {
            ContextualNetAddress::new(mock(), mock())
        }
    }

    impl Mock for RpcPeerInfo {
        fn mock() -> Self {
            RpcPeerInfo {
                id: mock(),
                address: mock(),
                last_ping_duration: mock(),
                is_outbound: mock(),
                time_offset: mock(),
                user_agent: "0.4.2".to_string(),
                advertised_protocol_version: mock(),
                time_connected: mock(),
                is_ibd_peer: mock(),
            }
        }
    }

    impl Mock for RpcMempoolEntry {
        fn mock() -> Self {
            RpcMempoolEntry { fee: mock(), transaction: mock(), is_orphan: mock() }
        }
    }

    impl Mock for RpcMempoolEntryByAddress {
        fn mock() -> Self {
            RpcMempoolEntryByAddress { address: mock(), sending: mock(), receiving: mock() }
        }
    }

    impl Mock for ScriptPublicKey {
        fn mock() -> Self {
            let mut bytes: [u8; 35] = [0; 35];
            rand::thread_rng().fill(&mut bytes[..]);
            ScriptPublicKey::from_vec(0, bytes.to_vec())
        }
    }

    impl Mock for RpcUtxoEntry {
        fn mock() -> Self {
            RpcUtxoEntry { amount: mock(), script_public_key: mock(), block_daa_score: mock(), is_coinbase: mock() }
        }
    }

    impl Mock for RpcOptionalUtxoEntry {
        fn mock() -> Self {
            RpcOptionalUtxoEntry {
                amount: mock(),
                script_public_key: mock(),
                block_daa_score: mock(),
                is_coinbase: mock(),
                verbose_data: mock(),
            }
        }
    }

    impl Mock for RpcOptionalUtxoEntryVerboseData {
        fn mock() -> Self {
            RpcOptionalUtxoEntryVerboseData { script_public_key_type: mock(), script_public_key_address: mock() }
        }
    }

    impl Mock for ScriptClass {
        fn mock() -> Self {
            match rand::thread_rng().r#gen::<u8>() % 4 {
                0 => ScriptClass::NonStandard,
                1 => ScriptClass::PubKey,
                2 => ScriptClass::PubKeyECDSA,
                _ => ScriptClass::ScriptHash, // 3
            }
        }
    }

    impl Mock for RpcTransactionOutpoint {
        fn mock() -> Self {
            RpcTransactionOutpoint { transaction_id: mock(), index: mock() }
        }
    }

    impl Mock for RpcUtxosByAddressesEntry {
        fn mock() -> Self {
            RpcUtxosByAddressesEntry { address: mock(), outpoint: mock(), utxo_entry: mock() }
        }
    }

    impl Mock for ProcessMetrics {
        fn mock() -> Self {
            ProcessMetrics {
                resident_set_size: mock(),
                virtual_memory_size: mock(),
                core_num: mock(),
                cpu_usage: mock(),
                fd_num: mock(),
                disk_io_read_bytes: mock(),
                disk_io_write_bytes: mock(),
                disk_io_read_per_sec: mock(),
                disk_io_write_per_sec: mock(),
            }
        }
    }

    impl Mock for ConnectionMetrics {
        fn mock() -> Self {
            ConnectionMetrics {
                borsh_live_connections: mock(),
                borsh_connection_attempts: mock(),
                borsh_handshake_failures: mock(),
                json_live_connections: mock(),
                json_connection_attempts: mock(),
                json_handshake_failures: mock(),
                active_peers: mock(),
            }
        }
    }

    impl Mock for BandwidthMetrics {
        fn mock() -> Self {
            BandwidthMetrics {
                borsh_bytes_tx: mock(),
                borsh_bytes_rx: mock(),
                json_bytes_tx: mock(),
                json_bytes_rx: mock(),
                p2p_bytes_tx: mock(),
                p2p_bytes_rx: mock(),
                grpc_bytes_tx: mock(),
                grpc_bytes_rx: mock(),
            }
        }
    }

    impl Mock for ConsensusMetrics {
        fn mock() -> Self {
            ConsensusMetrics {
                node_blocks_submitted_count: mock(),
                node_headers_processed_count: mock(),
                node_dependencies_processed_count: mock(),
                node_bodies_processed_count: mock(),
                node_transactions_processed_count: mock(),
                node_chain_blocks_processed_count: mock(),
                node_mass_processed_count: mock(),
                node_database_blocks_count: mock(),
                node_database_headers_count: mock(),
                network_mempool_size: mock(),
                network_tip_hashes_count: mock(),
                network_difficulty: mock(),
                network_past_median_time: mock(),
                network_virtual_parent_hashes_count: mock(),
                network_virtual_daa_score: mock(),
            }
        }
    }

    impl Mock for StorageMetrics {
        fn mock() -> Self {
            StorageMetrics { storage_size_bytes: mock() }
        }
    }

    // --------------------------------------------
    // implementations for all the rpc request
    // and response data structures.

    impl Mock for SubmitBlockRequest {
        fn mock() -> Self {
            SubmitBlockRequest { block: mock(), allow_non_daa_blocks: true }
        }
    }

    test!(SubmitBlockRequest);

    impl Mock for SubmitBlockResponse {
        fn mock() -> Self {
            SubmitBlockResponse { report: SubmitBlockReport::Success }
        }
    }

    test!(SubmitBlockResponse);

    impl Mock for GetBlockTemplateRequest {
        fn mock() -> Self {
            GetBlockTemplateRequest { pay_address: mock(), extra_data: vec![4, 2] }
        }
    }

    test!(GetBlockTemplateRequest);

    impl Mock for GetBlockTemplateResponse {
        fn mock() -> Self {
            GetBlockTemplateResponse { block: mock(), is_synced: true }
        }
    }

    test!(GetBlockTemplateResponse);

    impl Mock for GetBlockRequest {
        fn mock() -> Self {
            GetBlockRequest { hash: mock(), include_transactions: true }
        }
    }

    test!(GetBlockRequest);

    impl Mock for GetBlockResponse {
        fn mock() -> Self {
            GetBlockResponse { block: mock() }
        }
    }

    test!(GetBlockResponse);

    impl Mock for GetInfoRequest {
        fn mock() -> Self {
            GetInfoRequest {}
        }
    }

    test!(GetInfoRequest);

    impl Mock for GetInfoResponse {
        fn mock() -> Self {
            GetInfoResponse {
                p2p_id: Hash::mock().to_string(),
                mempool_size: mock(),
                server_version: "0.4.2".to_string(),
                is_utxo_indexed: true,
                is_synced: false,
                has_notify_command: true,
                has_message_id: false,
            }
        }
    }

    test!(GetInfoResponse);

    impl Mock for GetCurrentNetworkRequest {
        fn mock() -> Self {
            GetCurrentNetworkRequest {}
        }
    }

    test!(GetCurrentNetworkRequest);

    impl Mock for GetCurrentNetworkResponse {
        fn mock() -> Self {
            GetCurrentNetworkResponse { network: NetworkType::Mainnet }
        }
    }

    test!(GetCurrentNetworkResponse);

    impl Mock for GetPeerAddressesRequest {
        fn mock() -> Self {
            GetPeerAddressesRequest {}
        }
    }

    test!(GetPeerAddressesRequest);

    impl Mock for GetPeerAddressesResponse {
        fn mock() -> Self {
            GetPeerAddressesResponse { known_addresses: mock(), banned_addresses: mock() }
        }
    }

    test!(GetPeerAddressesResponse);

    impl Mock for GetSinkRequest {
        fn mock() -> Self {
            GetSinkRequest {}
        }
    }

    test!(GetSinkRequest);

    impl Mock for GetSinkResponse {
        fn mock() -> Self {
            GetSinkResponse { sink: mock() }
        }
    }

    test!(GetSinkResponse);

    impl Mock for GetMempoolEntryRequest {
        fn mock() -> Self {
            GetMempoolEntryRequest { transaction_id: mock(), include_orphan_pool: true, filter_transaction_pool: false }
        }
    }

    test!(GetMempoolEntryRequest);

    impl Mock for GetMempoolEntryResponse {
        fn mock() -> Self {
            GetMempoolEntryResponse { mempool_entry: RpcMempoolEntry { fee: mock(), transaction: mock(), is_orphan: false } }
        }
    }

    test!(GetMempoolEntryResponse);

    impl Mock for GetMempoolEntriesRequest {
        fn mock() -> Self {
            GetMempoolEntriesRequest { include_orphan_pool: true, filter_transaction_pool: false }
        }
    }

    test!(GetMempoolEntriesRequest);

    impl Mock for GetMempoolEntriesResponse {
        fn mock() -> Self {
            GetMempoolEntriesResponse { mempool_entries: mock() }
        }
    }

    test!(GetMempoolEntriesResponse);

    impl Mock for GetConnectedPeerInfoRequest {
        fn mock() -> Self {
            GetConnectedPeerInfoRequest {}
        }
    }

    test!(GetConnectedPeerInfoRequest);

    impl Mock for GetConnectedPeerInfoResponse {
        fn mock() -> Self {
            GetConnectedPeerInfoResponse { peer_info: mock() }
        }
    }

    test!(GetConnectedPeerInfoResponse);

    impl Mock for AddPeerRequest {
        fn mock() -> Self {
            AddPeerRequest { peer_address: mock(), is_permanent: mock() }
        }
    }

    test!(AddPeerRequest);

    impl Mock for AddPeerResponse {
        fn mock() -> Self {
            AddPeerResponse {}
        }
    }

    test!(AddPeerResponse);

    impl Mock for SubmitTransactionRequest {
        fn mock() -> Self {
            SubmitTransactionRequest { transaction: mock(), allow_orphan: mock() }
        }
    }

    test!(SubmitTransactionRequest);

    impl Mock for SubmitTransactionResponse {
        fn mock() -> Self {
            SubmitTransactionResponse { transaction_id: mock() }
        }
    }

    test!(SubmitTransactionResponse);

    impl Mock for GetSubnetworkRequest {
        fn mock() -> Self {
            GetSubnetworkRequest { subnetwork_id: mock() }
        }
    }

    test!(GetSubnetworkRequest);

    impl Mock for GetSubnetworkResponse {
        fn mock() -> Self {
            GetSubnetworkResponse { gas_limit: mock() }
        }
    }

    test!(GetSubnetworkResponse);

    impl Mock for GetVirtualChainFromBlockRequest {
        fn mock() -> Self {
            GetVirtualChainFromBlockRequest {
                start_hash: mock(),
                include_accepted_transaction_ids: mock(),
                min_confirmation_count: mock(),
            }
        }
    }

    test!(GetVirtualChainFromBlockRequest);

    impl Mock for RpcAcceptedTransactionIds {
        fn mock() -> Self {
            RpcAcceptedTransactionIds { accepting_block_hash: mock(), accepted_transaction_ids: mock() }
        }
    }

    impl Mock for GetVirtualChainFromBlockResponse {
        fn mock() -> Self {
            GetVirtualChainFromBlockResponse {
                removed_chain_block_hashes: mock(),
                added_chain_block_hashes: mock(),
                accepted_transaction_ids: mock(),
            }
        }
    }

    test!(GetVirtualChainFromBlockResponse);

    impl Mock for GetBlocksRequest {
        fn mock() -> Self {
            GetBlocksRequest { low_hash: mock(), include_blocks: mock(), include_transactions: mock() }
        }
    }

    test!(GetBlocksRequest);

    impl Mock for GetBlocksResponse {
        fn mock() -> Self {
            GetBlocksResponse { block_hashes: mock(), blocks: mock() }
        }
    }

    test!(GetBlocksResponse);

    impl Mock for GetBlockCountRequest {
        fn mock() -> Self {
            GetBlockCountRequest {}
        }
    }

    test!(GetBlockCountRequest);

    impl Mock for BlockCount {
        fn mock() -> Self {
            BlockCount { header_count: mock(), block_count: mock() }
        }
    }

    test!(BlockCount);

    impl Mock for GetBlockDagInfoRequest {
        fn mock() -> Self {
            GetBlockDagInfoRequest {}
        }
    }

    test!(GetBlockDagInfoRequest);

    impl Mock for GetBlockDagInfoResponse {
        fn mock() -> Self {
            GetBlockDagInfoResponse {
                network: NetworkType::Mainnet.try_into().unwrap(),
                block_count: mock(),
                header_count: mock(),
                tip_hashes: mock(),
                difficulty: mock(),
                past_median_time: mock(),
                virtual_parent_hashes: mock(),
                pruning_point_hash: mock(),
                virtual_daa_score: mock(),
                sink: mock(),
            }
        }
    }

    test!(GetBlockDagInfoResponse);

    impl Mock for ResolveFinalityConflictRequest {
        fn mock() -> Self {
            ResolveFinalityConflictRequest { finality_block_hash: mock() }
        }
    }

    test!(ResolveFinalityConflictRequest);

    impl Mock for ResolveFinalityConflictResponse {
        fn mock() -> Self {
            ResolveFinalityConflictResponse {}
        }
    }

    test!(ResolveFinalityConflictResponse);

    impl Mock for ShutdownRequest {
        fn mock() -> Self {
            ShutdownRequest {}
        }
    }

    test!(ShutdownRequest);

    impl Mock for ShutdownResponse {
        fn mock() -> Self {
            ShutdownResponse {}
        }
    }

    test!(ShutdownResponse);

    impl Mock for GetHeadersRequest {
        fn mock() -> Self {
            GetHeadersRequest { start_hash: mock(), limit: mock(), is_ascending: mock() }
        }
    }

    test!(GetHeadersRequest);

    impl Mock for GetHeadersResponse {
        fn mock() -> Self {
            GetHeadersResponse { headers: mock() }
        }
    }

    test!(GetHeadersResponse);

    impl Mock for GetBalanceByAddressRequest {
        fn mock() -> Self {
            GetBalanceByAddressRequest { address: mock() }
        }
    }

    test!(GetBalanceByAddressRequest);

    impl Mock for GetBalanceByAddressResponse {
        fn mock() -> Self {
            GetBalanceByAddressResponse { balance: mock() }
        }
    }

    test!(GetBalanceByAddressResponse);

    impl Mock for GetBalancesByAddressesRequest {
        fn mock() -> Self {
            GetBalancesByAddressesRequest { addresses: mock() }
        }
    }

    test!(GetBalancesByAddressesRequest);

    impl Mock for RpcBalancesByAddressesEntry {
        fn mock() -> Self {
            RpcBalancesByAddressesEntry { address: mock(), balance: mock() }
        }
    }

    impl Mock for GetBalancesByAddressesResponse {
        fn mock() -> Self {
            GetBalancesByAddressesResponse { entries: mock() }
        }
    }

    test!(GetBalancesByAddressesResponse);

    impl Mock for GetSinkBlueScoreRequest {
        fn mock() -> Self {
            GetSinkBlueScoreRequest {}
        }
    }

    test!(GetSinkBlueScoreRequest);

    impl Mock for GetSinkBlueScoreResponse {
        fn mock() -> Self {
            GetSinkBlueScoreResponse { blue_score: mock() }
        }
    }

    test!(GetSinkBlueScoreResponse);

    impl Mock for GetUtxosByAddressesRequest {
        fn mock() -> Self {
            GetUtxosByAddressesRequest { addresses: mock() }
        }
    }

    test!(GetUtxosByAddressesRequest);

    impl Mock for GetUtxosByAddressesResponse {
        fn mock() -> Self {
            GetUtxosByAddressesResponse { entries: mock() }
        }
    }

    test!(GetUtxosByAddressesResponse);

    impl Mock for BanRequest {
        fn mock() -> Self {
            BanRequest { ip: mock() }
        }
    }

    test!(BanRequest);

    impl Mock for BanResponse {
        fn mock() -> Self {
            BanResponse {}
        }
    }

    test!(BanResponse);

    impl Mock for UnbanRequest {
        fn mock() -> Self {
            UnbanRequest { ip: mock() }
        }
    }

    test!(UnbanRequest);

    impl Mock for UnbanResponse {
        fn mock() -> Self {
            UnbanResponse {}
        }
    }

    test!(UnbanResponse);

    impl Mock for EstimateNetworkHashesPerSecondRequest {
        fn mock() -> Self {
            EstimateNetworkHashesPerSecondRequest { window_size: mock(), start_hash: mock() }
        }
    }

    test!(EstimateNetworkHashesPerSecondRequest);

    impl Mock for EstimateNetworkHashesPerSecondResponse {
        fn mock() -> Self {
            EstimateNetworkHashesPerSecondResponse { network_hashes_per_second: mock() }
        }
    }

    test!(EstimateNetworkHashesPerSecondResponse);

    impl Mock for GetMempoolEntriesByAddressesRequest {
        fn mock() -> Self {
            GetMempoolEntriesByAddressesRequest { addresses: mock(), include_orphan_pool: true, filter_transaction_pool: false }
        }
    }

    test!(GetMempoolEntriesByAddressesRequest);

    impl Mock for GetMempoolEntriesByAddressesResponse {
        fn mock() -> Self {
            GetMempoolEntriesByAddressesResponse { entries: mock() }
        }
    }

    test!(GetMempoolEntriesByAddressesResponse);

    impl Mock for GetCoinSupplyRequest {
        fn mock() -> Self {
            GetCoinSupplyRequest {}
        }
    }

    test!(GetCoinSupplyRequest);

    impl Mock for GetCoinSupplyResponse {
        fn mock() -> Self {
            GetCoinSupplyResponse { max_sompi: mock(), circulating_sompi: mock() }
        }
    }

    test!(GetCoinSupplyResponse);

    impl Mock for PingRequest {
        fn mock() -> Self {
            PingRequest {}
        }
    }

    test!(PingRequest);

    impl Mock for PingResponse {
        fn mock() -> Self {
            PingResponse {}
        }
    }

    test!(PingResponse);

    impl Mock for GetConnectionsRequest {
        fn mock() -> Self {
            GetConnectionsRequest { include_profile_data: false }
        }
    }

    test!(GetConnectionsRequest);

    impl Mock for GetConnectionsResponse {
        fn mock() -> Self {
            GetConnectionsResponse { clients: mock(), peers: mock(), profile_data: None }
        }
    }

    test!(GetConnectionsResponse);

    impl Mock for GetSystemInfoRequest {
        fn mock() -> Self {
            GetSystemInfoRequest {}
        }
    }

    test!(GetSystemInfoRequest);

    impl Mock for GetSystemInfoResponse {
        fn mock() -> Self {
            GetSystemInfoResponse {
                version: "1.2.3".to_string(),
                system_id: mock(),
                git_hash: mock(),
                cpu_physical_cores: mock(),
                total_memory: mock(),
                fd_limit: mock(),
                proxy_socket_limit_per_cpu_core: mock(),
            }
        }
    }

    test!(GetSystemInfoResponse);

    impl Mock for GetMetricsRequest {
        fn mock() -> Self {
            GetMetricsRequest {
                process_metrics: true,
                connection_metrics: true,
                bandwidth_metrics: true,
                consensus_metrics: true,
                storage_metrics: true,
                custom_metrics: false,
            }
        }
    }

    test!(GetMetricsRequest);

    impl Mock for GetMetricsResponse {
        fn mock() -> Self {
            GetMetricsResponse {
                server_time: mock(),
                process_metrics: mock(),
                connection_metrics: mock(),
                bandwidth_metrics: mock(),
                consensus_metrics: mock(),
                storage_metrics: mock(),
                custom_metrics: None,
            }
        }
    }

    test!(GetMetricsResponse);

    impl Mock for GetServerInfoRequest {
        fn mock() -> Self {
            GetServerInfoRequest {}
        }
    }

    test!(GetServerInfoRequest);

    impl Mock for GetServerInfoResponse {
        fn mock() -> Self {
            GetServerInfoResponse {
                rpc_api_version: mock(),
                rpc_api_revision: mock(),
                server_version: "0.4.2".to_string(),
                network_id: NetworkType::Mainnet.try_into().unwrap(),
                has_utxo_index: true,
                is_synced: false,
                virtual_daa_score: mock(),
            }
        }
    }

    test!(GetServerInfoResponse);

    impl Mock for GetSyncStatusRequest {
        fn mock() -> Self {
            GetSyncStatusRequest {}
        }
    }

    test!(GetSyncStatusRequest);

    impl Mock for GetSyncStatusResponse {
        fn mock() -> Self {
            GetSyncStatusResponse { is_synced: true }
        }
    }

    test!(GetSyncStatusResponse);

    impl Mock for GetDaaScoreTimestampEstimateRequest {
        fn mock() -> Self {
            GetDaaScoreTimestampEstimateRequest { daa_scores: mock() }
        }
    }

    test!(GetDaaScoreTimestampEstimateRequest);

    impl Mock for GetDaaScoreTimestampEstimateResponse {
        fn mock() -> Self {
            GetDaaScoreTimestampEstimateResponse { timestamps: mock() }
        }
    }

    test!(GetDaaScoreTimestampEstimateResponse);

    impl Mock for GetVirtualChainFromBlockV2Request {
        fn mock() -> Self {
            GetVirtualChainFromBlockV2Request { start_hash: mock(), data_verbosity_level: None, min_confirmation_count: mock() }
        }
    }

    test!(GetVirtualChainFromBlockV2Request);

    impl Mock for RpcChainBlockAcceptedTransactions {
        fn mock() -> Self {
            RpcChainBlockAcceptedTransactions { chain_block_header: mock(), accepted_transactions: mock() }
        }
    }

    impl Mock for GetVirtualChainFromBlockV2Response {
        fn mock() -> Self {
            GetVirtualChainFromBlockV2Response {
                removed_chain_block_hashes: mock(),
                added_chain_block_hashes: mock(),
                chain_block_accepted_transactions: mock(),
            }
        }
    }

    test!(GetVirtualChainFromBlockV2Response);

    impl Mock for NotifyBlockAddedRequest {
        fn mock() -> Self {
            NotifyBlockAddedRequest { command: Command::Start }
        }
    }

    test!(NotifyBlockAddedRequest);

    impl Mock for NotifyBlockAddedResponse {
        fn mock() -> Self {
            NotifyBlockAddedResponse {}
        }
    }

    test!(NotifyBlockAddedResponse);

    impl Mock for BlockAddedNotification {
        fn mock() -> Self {
            BlockAddedNotification { block: mock() }
        }
    }

    test!(BlockAddedNotification);

    impl Mock for NotifyVirtualChainChangedRequest {
        fn mock() -> Self {
            NotifyVirtualChainChangedRequest { command: Command::Start, include_accepted_transaction_ids: true }
        }
    }

    test!(NotifyVirtualChainChangedRequest);

    impl Mock for NotifyVirtualChainChangedResponse {
        fn mock() -> Self {
            NotifyVirtualChainChangedResponse {}
        }
    }

    test!(NotifyVirtualChainChangedResponse);

    impl Mock for VirtualChainChangedNotification {
        fn mock() -> Self {
            VirtualChainChangedNotification {
                removed_chain_block_hashes: mock(),
                added_chain_block_hashes: mock(),
                accepted_transaction_ids: mock(),
            }
        }
    }

    test!(VirtualChainChangedNotification);

    impl Mock for NotifyFinalityConflictRequest {
        fn mock() -> Self {
            NotifyFinalityConflictRequest { command: Command::Start }
        }
    }

    test!(NotifyFinalityConflictRequest);

    impl Mock for NotifyFinalityConflictResponse {
        fn mock() -> Self {
            NotifyFinalityConflictResponse {}
        }
    }

    test!(NotifyFinalityConflictResponse);

    impl Mock for FinalityConflictNotification {
        fn mock() -> Self {
            FinalityConflictNotification { violating_block_hash: mock() }
        }
    }

    test!(FinalityConflictNotification);

    impl Mock for NotifyFinalityConflictResolvedRequest {
        fn mock() -> Self {
            NotifyFinalityConflictResolvedRequest { command: Command::Start }
        }
    }

    test!(NotifyFinalityConflictResolvedRequest);

    impl Mock for NotifyFinalityConflictResolvedResponse {
        fn mock() -> Self {
            NotifyFinalityConflictResolvedResponse {}
        }
    }

    test!(NotifyFinalityConflictResolvedResponse);

    impl Mock for FinalityConflictResolvedNotification {
        fn mock() -> Self {
            FinalityConflictResolvedNotification { finality_block_hash: mock() }
        }
    }

    test!(FinalityConflictResolvedNotification);

    impl Mock for NotifyUtxosChangedRequest {
        fn mock() -> Self {
            NotifyUtxosChangedRequest { addresses: mock(), command: Command::Start }
        }
    }

    test!(NotifyUtxosChangedRequest);

    impl Mock for NotifyUtxosChangedResponse {
        fn mock() -> Self {
            NotifyUtxosChangedResponse {}
        }
    }

    test!(NotifyUtxosChangedResponse);

    impl Mock for UtxosChangedNotification {
        fn mock() -> Self {
            UtxosChangedNotification { added: mock(), removed: mock() }
        }
    }

    test!(UtxosChangedNotification);

    impl Mock for NotifySinkBlueScoreChangedRequest {
        fn mock() -> Self {
            NotifySinkBlueScoreChangedRequest { command: Command::Start }
        }
    }

    test!(NotifySinkBlueScoreChangedRequest);

    impl Mock for NotifySinkBlueScoreChangedResponse {
        fn mock() -> Self {
            NotifySinkBlueScoreChangedResponse {}
        }
    }

    test!(NotifySinkBlueScoreChangedResponse);

    impl Mock for SinkBlueScoreChangedNotification {
        fn mock() -> Self {
            SinkBlueScoreChangedNotification { sink_blue_score: mock() }
        }
    }

    test!(SinkBlueScoreChangedNotification);

    impl Mock for NotifyVirtualDaaScoreChangedRequest {
        fn mock() -> Self {
            NotifyVirtualDaaScoreChangedRequest { command: Command::Start }
        }
    }

    test!(NotifyVirtualDaaScoreChangedRequest);

    impl Mock for NotifyVirtualDaaScoreChangedResponse {
        fn mock() -> Self {
            NotifyVirtualDaaScoreChangedResponse {}
        }
    }

    test!(NotifyVirtualDaaScoreChangedResponse);

    impl Mock for VirtualDaaScoreChangedNotification {
        fn mock() -> Self {
            VirtualDaaScoreChangedNotification { virtual_daa_score: mock() }
        }
    }

    test!(VirtualDaaScoreChangedNotification);

    impl Mock for NotifyPruningPointUtxoSetOverrideRequest {
        fn mock() -> Self {
            NotifyPruningPointUtxoSetOverrideRequest { command: Command::Start }
        }
    }

    test!(NotifyPruningPointUtxoSetOverrideRequest);

    impl Mock for NotifyPruningPointUtxoSetOverrideResponse {
        fn mock() -> Self {
            NotifyPruningPointUtxoSetOverrideResponse {}
        }
    }

    test!(NotifyPruningPointUtxoSetOverrideResponse);

    impl Mock for PruningPointUtxoSetOverrideNotification {
        fn mock() -> Self {
            PruningPointUtxoSetOverrideNotification {}
        }
    }

    test!(PruningPointUtxoSetOverrideNotification);

    impl Mock for NotifyNewBlockTemplateRequest {
        fn mock() -> Self {
            NotifyNewBlockTemplateRequest { command: Command::Start }
        }
    }

    test!(NotifyNewBlockTemplateRequest);

    impl Mock for NotifyNewBlockTemplateResponse {
        fn mock() -> Self {
            NotifyNewBlockTemplateResponse {}
        }
    }

    test!(NotifyNewBlockTemplateResponse);

    impl Mock for NewBlockTemplateNotification {
        fn mock() -> Self {
            NewBlockTemplateNotification {}
        }
    }

    test!(NewBlockTemplateNotification);

    impl Mock for NotifyPalwClassReadinessChangedRequest {
        fn mock() -> Self {
            NotifyPalwClassReadinessChangedRequest { command: Command::Start }
        }
    }
    test!(NotifyPalwClassReadinessChangedRequest);
    impl Mock for NotifyPalwClassReadinessChangedResponse {
        fn mock() -> Self {
            NotifyPalwClassReadinessChangedResponse {}
        }
    }
    test!(NotifyPalwClassReadinessChangedResponse);
    impl Mock for PalwClassReadinessChangedNotification {
        fn mock() -> Self {
            PalwClassReadinessChangedNotification {
                class_id: mock_hex(),
                model_name: "QWEN36".to_string(),
                registry_state: "Probation".to_string(),
                previous_registry_state: "Prefetching".to_string(),
                ready_seats: 7,
                previous_ready_seats: 6,
                required_ready_seats: 7,
                bonded_seats: 8,
            }
        }
    }
    test!(PalwClassReadinessChangedNotification);

    impl Mock for NotifyPalwPanelAssignmentRequest {
        fn mock() -> Self {
            NotifyPalwPanelAssignmentRequest { command: Command::Start }
        }
    }
    test!(NotifyPalwPanelAssignmentRequest);
    impl Mock for NotifyPalwPanelAssignmentResponse {
        fn mock() -> Self {
            NotifyPalwPanelAssignmentResponse {}
        }
    }
    test!(NotifyPalwPanelAssignmentResponse);
    impl Mock for PalwPanelAssignmentNotification {
        fn mock() -> Self {
            PalwPanelAssignmentNotification {
                claim_id: mock_hex(),
                class_id: mock_hex(),
                licensed_state: "panelBound".to_string(),
                deadline_daa: mock(),
                coverage_mask: mock(),
                full_seat: format!("{}:0", mock_hex()),
                valid_receipt_seats: mock(),
                selected_panel_seats: mock(),
                seats: mock(),
            }
        }
    }
    test!(PalwPanelAssignmentNotification);

    impl Mock for NotifyPalwPanelReceiptRequest {
        fn mock() -> Self {
            NotifyPalwPanelReceiptRequest { command: Command::Start }
        }
    }
    test!(NotifyPalwPanelReceiptRequest);
    impl Mock for NotifyPalwPanelReceiptResponse {
        fn mock() -> Self {
            NotifyPalwPanelReceiptResponse {}
        }
    }
    test!(NotifyPalwPanelReceiptResponse);
    impl Mock for PalwPanelReceiptNotification {
        fn mock() -> Self {
            PalwPanelReceiptNotification {
                claim_id: mock_hex(),
                class_id: mock_hex(),
                coverage_mask: mock(),
                previous_coverage_mask: mock(),
                valid_receipt_seats: mock(),
                previous_valid_receipt_seats: mock(),
                selected_panel_seats: mock(),
            }
        }
    }
    test!(PalwPanelReceiptNotification);

    impl Mock for NotifyPalwPanelEligibilityChangedRequest {
        fn mock() -> Self {
            NotifyPalwPanelEligibilityChangedRequest { command: Command::Start }
        }
    }
    test!(NotifyPalwPanelEligibilityChangedRequest);
    impl Mock for NotifyPalwPanelEligibilityChangedResponse {
        fn mock() -> Self {
            NotifyPalwPanelEligibilityChangedResponse {}
        }
    }
    test!(NotifyPalwPanelEligibilityChangedResponse);
    impl Mock for PalwPanelEligibilityChangedNotification {
        fn mock() -> Self {
            PalwPanelEligibilityChangedNotification {
                seat_id: format!("{}:0", mock_hex()),
                class_id: mock_hex(),
                eligible: mock(),
                ready: mock(),
                hold: mock(),
            }
        }
    }
    test!(PalwPanelEligibilityChangedNotification);

    impl Mock for SubscribeResponse {
        fn mock() -> Self {
            SubscribeResponse::new(mock())
        }
    }

    test!(SubscribeResponse);

    impl Mock for UnsubscribeResponse {
        fn mock() -> Self {
            UnsubscribeResponse {}
        }
    }

    test!(UnsubscribeResponse);

    // ops 182–184: the settlement read, the precommit duty and the class contexts round-trip.
    fn mock_hex() -> String {
        format!("{:016x}{:016x}", mock::<u64>(), mock::<u64>())
    }

    impl Mock for GetPalwSettlementRequest {
        fn mock() -> Self {
            GetPalwSettlementRequest { daa_score: mock() }
        }
    }

    test!(GetPalwSettlementRequest);

    impl Mock for GetPalwSettlementResponse {
        fn mock() -> Self {
            GetPalwSettlementResponse {
                available: mock(),
                sink_daa: mock(),
                daa_score: mock(),
                settled: mock(),
                depth: mock(),
                pending_anchors: mock(),
                depth_is_lower_bound: mock(),
                safe_frontier_blue_score: mock(),
                safe_frontier_daa: mock(),
            }
        }
    }

    test!(GetPalwSettlementResponse);

    impl Mock for GetPrecommitDutyRequest {
        fn mock() -> Self {
            GetPrecommitDutyRequest { validator_id: mock_hex(), bond_outpoint: format!("{}:{}", mock_hex(), mock::<u32>()) }
        }
    }

    test!(GetPrecommitDutyRequest);

    impl Mock for RpcPrecommitDue {
        fn mock() -> Self {
            RpcPrecommitDue { epoch: mock(), anchor_hash: mock_hex(), anchor_daa_score: mock(), snapshot_commitment: mock_hex() }
        }
    }

    test!(RpcPrecommitDue);

    impl Mock for GetPrecommitDutyResponse {
        fn mock() -> Self {
            GetPrecommitDutyResponse {
                available: mock(),
                round_active: mock(),
                sink_daa_score: mock(),
                held_epoch: mock(),
                held_anchor: mock_hex(),
                due: mock(),
            }
        }
    }

    test!(GetPrecommitDutyResponse);

    impl Mock for GetPalwClassContextsRequest {
        fn mock() -> Self {
            GetPalwClassContextsRequest {}
        }
    }

    test!(GetPalwClassContextsRequest);

    impl Mock for RpcPalwClassContext {
        fn mock() -> Self {
            RpcPalwClassContext {
                class_id: mock_hex(),
                model_id: mock_hex(),
                n_ctx: mock(),
                canonical_prefill_tokens: mock(),
                canonical_decode_tokens: mock(),
                canonical_footprint_positions: mock(),
                max_context_tokens: mock(),
                source: "chain_registration".to_string(),
            }
        }
    }

    test!(RpcPalwClassContext);

    /// The row a client reads for this build's Qwen3.6-35B-A3B class — a (7, 2) canonical job at
    /// `n_ctx` 8, whose footprint is 8 — survives the wire field for field.
    #[test]
    fn a_class_context_row_round_trips_with_its_footprint() {
        let row = RpcPalwClassContext {
            class_id: "11".repeat(64),
            model_id: "qwen3.6-35b-a3b".to_string(),
            n_ctx: 8,
            canonical_prefill_tokens: 7,
            canonical_decode_tokens: 2,
            canonical_footprint_positions: 8,
            max_context_tokens: 8,
            source: "chain_registration".to_string(),
        };
        let mut bytes = Vec::new();
        serialize!(RpcPalwClassContext, &row, &mut bytes).unwrap();
        let back = deserialize!(RpcPalwClassContext, &mut bytes.as_slice()).unwrap();
        assert_eq!((back.class_id, back.model_id, back.source), (row.class_id.clone(), row.model_id.clone(), row.source.clone()));
        assert_eq!(
            (
                back.n_ctx,
                back.canonical_prefill_tokens,
                back.canonical_decode_tokens,
                back.canonical_footprint_positions,
                back.max_context_tokens
            ),
            (8, 7, 2, 8, 8)
        );
    }

    impl Mock for GetPalwClassContextsResponse {
        fn mock() -> Self {
            GetPalwClassContextsResponse {
                available: mock(),
                fp_max_prompt_tokens: mock(),
                fp_max_decode_tokens: mock(),
                classes: mock(),
            }
        }
    }

    test!(GetPalwClassContextsResponse);

    impl Mock for GetPalwClassEconomicsRequest {
        fn mock() -> Self {
            GetPalwClassEconomicsRequest {}
        }
    }

    test!(GetPalwClassEconomicsRequest);

    impl Mock for RpcPalwClassLedgerTotals {
        fn mock() -> Self {
            RpcPalwClassLedgerTotals {
                available: mock(),
                claims: mock(),
                bound: mock(),
                licensed: mock(),
                finals: mock(),
                voided: mock(),
                redrawn: mock(),
                paid_at_acceptance: mock(),
                escrow_final_sompi: "1".to_string(),
                producer_paid_sompi: "1".to_string(),
                panel_paid_sompi: "1".to_string(),
                reserve_sompi: "1".to_string(),
                burned_sompi: "1".to_string(),
                attempted_compute: "1".to_string(),
                final_compute: "1".to_string(),
                verification_compute: "1".to_string(),
                producer_per_attempted_compute: "1".to_string(),
                panel_per_verification_compute: "1".to_string(),
                total_per_attempted_compute: "1".to_string(),
                total_per_final_compute: "1".to_string(),
                licence_rate_permille: mock(),
                final_of_licensed_permille: mock(),
                final_rate_permille: mock(),
                avg_bind_wait_daa: mock(),
                avg_licence_wait_daa: mock(),
                avg_final_wait_daa: mock(),
                avg_void_wait_daa: mock(),
                avg_expected_attempts_q32: "1".to_string(),
                avg_network_expected_attempts_q32: "1".to_string(),
                first_accepted_daa: mock(),
                last_accepted_daa: mock(),
            }
        }
    }

    test!(RpcPalwClassLedgerTotals);

    impl Mock for RpcPalwClassNodeTelemetry {
        fn mock() -> Self {
            RpcPalwClassNodeTelemetry {
                available: mock(),
                draws: mock(),
                class_wins: mock(),
                produced: mock(),
                draw_millis: mock(),
                storage_read_mib: mock(),
                replays: mock(),
                replay_millis: mock(),
                replay_leaves: mock(),
                receipts_valid: mock(),
                receipts_unavailable: mock(),
                receipts_incapable: mock(),
                receipts_other: mock(),
                openings_held: mock(),
            }
        }
    }

    test!(RpcPalwClassNodeTelemetry);

    impl Mock for RpcPalwClassEconomics {
        fn mock() -> Self {
            RpcPalwClassEconomics {
                class_id: mock_hex(),
                model_id: mock_hex(),
                is_base_class: mock(),
                status: "Active".to_string(),
                share_permille: mock(),
                pwu_per_inference: mock(),
                class_target: u128::MAX.to_string(),
                expected_attempts: mock(),
                expected_attempts_q32: (1u128 << 32).to_string(),
                economic_compute_job: "18055200736".to_string(),
                economic_compute_canonical: "21070759296".to_string(),
                economic_source: "build_ledger".to_string(),
                claims_accepted: mock(),
                claims_provisional: mock(),
                claims_panel_bound: mock(),
                claims_licensed: mock(),
                claims_final: mock(),
                claims_voided: mock(),
                claims_redrawn: mock(),
                escrow_accepted_sompi: "320084650080".to_string(),
                escrow_final_sompi: "0".to_string(),
                ledger: mock(),
                telemetry: mock(),
                eligible_seats: mock(),
                duty_seats_inflight: mock(),
                seat_exposure_inflight_sompi: "0".to_string(),
                free_collateral_sompi: "1000000000000".to_string(),
            }
        }
    }

    test!(RpcPalwClassEconomics);

    impl Mock for GetPalwClassEconomicsResponse {
        fn mock() -> Self {
            GetPalwClassEconomicsResponse {
                available: mock(),
                tip_daa: mock(),
                economic_compute_version: mock(),
                seat_count: mock(),
                prefill_draw: mock(),
                network_bits: mock(),
                network_expected_attempts_q32: "8589934592".to_string(),
                ledger_available: mock(),
                ledger_claims: mock(),
                ledger_first_daa: mock(),
                ledger_last_daa: mock(),
                classes: mock(),
            }
        }
    }

    test!(GetPalwClassEconomicsResponse);

    impl Mock for GetPalwModelRegistryRequest {
        fn mock() -> Self {
            GetPalwModelRegistryRequest {}
        }
    }

    test!(GetPalwModelRegistryRequest);

    impl Mock for GetPalwFreePromptPriceRequest {
        fn mock() -> Self {
            GetPalwFreePromptPriceRequest {
                class_id: mock_hex(),
                prompt_token_ids: vec![151644, 872, 198, 9707],
                prompt_tokens: mock(),
                decode_tokens_executed: mock(),
                work_leaves: mock(),
                bond: format!("{}:0", mock_hex()),
            }
        }
    }

    test!(GetPalwFreePromptPriceRequest);

    impl Mock for GetPalwFreePromptPriceResponse {
        fn mock() -> Self {
            GetPalwFreePromptPriceResponse {
                available: mock(),
                daa_score: mock(),
                priced: mock(),
                refusal: "FreePromptWorkLeavesMismatch".to_string(),
                priced_in_compute: mock(),
                quanta: mock(),
                pwu: mock(),
                reserved_sompi: "340282366920938463463374607431768211455".to_string(),
                bond_room_sompi: "1000000000000".to_string(),
            }
        }
    }

    test!(GetPalwFreePromptPriceResponse);

    impl Mock for RpcPalwPanelHoldReason {
        fn mock() -> Self {
            RpcPalwPanelHoldReason { code: "NO_ARTIFACT".to_string(), message: "this node does not hold a converted artifact".to_string() }
        }
    }
    test!(RpcPalwPanelHoldReason);

    impl Mock for RpcPalwPanelHoldReasonCount {
        fn mock() -> Self {
            RpcPalwPanelHoldReasonCount { code: "NO_ARTIFACT".to_string(), message: "this node does not hold a converted artifact".to_string(), seats: mock() }
        }
    }
    test!(RpcPalwPanelHoldReasonCount);

    impl Mock for RpcPalwClassPanelStatus {
        fn mock() -> Self {
            RpcPalwClassPanelStatus {
                class_id: mock_hex(),
                model_name: "QWEN36".to_string(),
                registry_state: "Probation".to_string(),
                bonded_seats: mock(),
                ready_seats: mock(),
                required_ready_seats: mock(),
                selected_panel_seats: mock(),
                valid_receipt_seats: mock(),
                panel_size: mock(),
                receipt_quorum: mock(),
                full_seats_per_panel: mock(),
                partial_seats_per_panel: mock(),
                segment_count: mock(),
                inflight_claims: mock(),
                active_assignments: mock(),
                admission_permille: mock(),
                verification_mode: "s1".to_string(),
                s1_active: mock(),
                s1_scheduled_daa: mock(),
                s3_active: mock(),
                s3_scheduled_daa: mock(),
                s2_active: mock(),
                s2_scheduled_daa: mock(),
                holds_local: mock(),
                missing: mock(),
            }
        }
    }
    test!(RpcPalwClassPanelStatus);

    impl Mock for RpcPalwPanelSeat {
        fn mock() -> Self {
            RpcPalwPanelSeat {
                seat_id: format!("{}:0", mock_hex()),
                bond_outpoint: format!("{}:0", mock_hex()),
                class_id: mock_hex(),
                ready: mock(),
                eligible: mock(),
                readiness_version: mock(),
                readiness_proved_daa: mock(),
                readiness_expires_daa: mock(),
                collateral_available: "124000000000".to_string(),
                collateral_locked: "0".to_string(),
                assigned: mock(),
                hold: mock(),
            }
        }
    }
    test!(RpcPalwPanelSeat);

    impl Mock for RpcPalwPanelAssignmentSeat {
        fn mock() -> Self {
            RpcPalwPanelAssignmentSeat {
                seat_id: format!("{}:0", mock_hex()),
                seat_index: mock(),
                full_seat: mock(),
                segment_index: mock(),
                mask: mock(),
                receipt_status: "valid".to_string(),
                credited_daa: mock(),
            }
        }
    }
    test!(RpcPalwPanelAssignmentSeat);

    impl Mock for RpcPalwPanelAssignment {
        fn mock() -> Self {
            RpcPalwPanelAssignment {
                claim_id: mock_hex(),
                class_id: mock_hex(),
                licensed_state: "panelBound".to_string(),
                deadline_daa: mock(),
                coverage_mask: mock(),
                full_seat: format!("{}:0", mock_hex()),
                valid_receipt_seats: mock(),
                selected_panel_seats: mock(),
                seats: mock(),
            }
        }
    }
    test!(RpcPalwPanelAssignment);

    impl Mock for RpcPalwLocalPanelClass {
        fn mock() -> Self {
            RpcPalwLocalPanelClass {
                class_id: mock_hex(),
                model_name: "QWEN36".to_string(),
                seat_id: format!("{}:2", mock_hex()),
                artifact_loaded: mock(),
                artifact_root: mock_hex(),
                working_set_bytes: mock(),
                replay_capable: mock(),
                synced: mock(),
                bond_active: mock(),
                collateral_sompi: mock(),
                readiness_proof_accepted: mock(),
                readiness_proved_daa: mock(),
                chain_state: "READY".to_string(),
                assignments: mock(),
                hold: mock(),
            }
        }
    }
    test!(RpcPalwLocalPanelClass);

    impl Mock for GetPalwClassPanelStatusRequest {
        fn mock() -> Self {
            GetPalwClassPanelStatusRequest { class_id: mock_hex() }
        }
    }
    test!(GetPalwClassPanelStatusRequest);

    impl Mock for GetPalwClassPanelStatusResponse {
        fn mock() -> Self {
            GetPalwClassPanelStatusResponse { available: mock(), tip_daa: mock(), found: mock(), status: mock() }
        }
    }
    test!(GetPalwClassPanelStatusResponse);

    impl Mock for GetPalwPanelSeatsRequest {
        fn mock() -> Self {
            GetPalwPanelSeatsRequest { class_id: mock_hex() }
        }
    }
    test!(GetPalwPanelSeatsRequest);

    impl Mock for GetPalwPanelSeatsResponse {
        fn mock() -> Self {
            GetPalwPanelSeatsResponse { available: mock(), tip_daa: mock(), seats: mock() }
        }
    }
    test!(GetPalwPanelSeatsResponse);

    impl Mock for GetPalwPanelStatusRequest {
        fn mock() -> Self {
            GetPalwPanelStatusRequest { class_id: mock_hex() }
        }
    }
    test!(GetPalwPanelStatusRequest);

    impl Mock for GetPalwPanelStatusResponse {
        fn mock() -> Self {
            GetPalwPanelStatusResponse {
                available: mock(),
                tip_daa: mock(),
                panel_running: mock(),
                panel_submitter: mock(),
                synced: mock(),
                classes: mock(),
            }
        }
    }
    test!(GetPalwPanelStatusResponse);

    impl Mock for GetPalwPanelAssignmentsRequest {
        fn mock() -> Self {
            GetPalwPanelAssignmentsRequest { claim_id: mock_hex(), seat_id: format!("{}:0", mock_hex()) }
        }
    }
    test!(GetPalwPanelAssignmentsRequest);

    impl Mock for GetPalwPanelAssignmentsResponse {
        fn mock() -> Self {
            GetPalwPanelAssignmentsResponse { available: mock(), tip_daa: mock(), truncated: mock(), assignments: mock() }
        }
    }
    test!(GetPalwPanelAssignmentsResponse);

    impl Mock for RpcPalwModelPreflightCheck {
        fn mock() -> Self {
            RpcPalwModelPreflightCheck { code: "FitsGlobalWindow".into(), ok: mock(), message: "fits".into() }
        }
    }
    test!(RpcPalwModelPreflightCheck);

    impl Mock for RpcPalwModelRegistration {
        fn mock() -> Self {
            RpcPalwModelRegistration {
                object_id: mock_hex(),
                class_id: mock_hex(),
                constructed: mock(),
                submitted: mock(),
                accepted: mock(),
                included: mock(),
                folded: mock(),
                submission_state: "folded".into(),
                processor_verdict: "ADMISSION_OK".into(),
                reject_code: String::new(),
                mempool_accepted: mock(),
                included_block: mock_hex(),
                included_daa: mock(),
                registry_state: "Probation".into(),
                transaction_id: mock_hex(),
            }
        }
    }
    test!(RpcPalwModelRegistration);

    impl Mock for GetPalwModelPreflightRequest {
        fn mock() -> Self {
            GetPalwModelPreflightRequest { object_hex: "00".into(), class_id: mock_hex() }
        }
    }
    test!(GetPalwModelPreflightRequest);

    impl Mock for GetPalwModelPreflightResponse {
        fn mock() -> Self {
            GetPalwModelPreflightResponse {
                available: mock(),
                tip_daa: mock(),
                class_id: mock_hex(),
                artifact_root: mock_hex(),
                n_ctx: mock(),
                layer_count: mock(),
                admissible: mock(),
                processor_verdict: "ADMISSION_OK".into(),
                reject_code: String::new(),
                checks: mock(),
            }
        }
    }
    test!(GetPalwModelPreflightResponse);

    impl Mock for SubmitPalwModelRegistrationRequest {
        fn mock() -> Self {
            SubmitPalwModelRegistrationRequest { object_hex: "00".into(), transaction_id: mock_hex() }
        }
    }
    test!(SubmitPalwModelRegistrationRequest);

    impl Mock for SubmitPalwModelRegistrationResponse {
        fn mock() -> Self {
            SubmitPalwModelRegistrationResponse { available: mock(), tip_daa: mock(), registration: mock(), checks: mock() }
        }
    }
    test!(SubmitPalwModelRegistrationResponse);

    impl Mock for GetPalwModelRegistrationStatusRequest {
        fn mock() -> Self {
            GetPalwModelRegistrationStatusRequest { class_id: mock_hex(), object_id: mock_hex(), transaction_id: mock_hex() }
        }
    }
    test!(GetPalwModelRegistrationStatusRequest);

    impl Mock for GetPalwModelRegistrationStatusResponse {
        fn mock() -> Self {
            GetPalwModelRegistrationStatusResponse { available: mock(), tip_daa: mock(), found: mock(), registration: mock() }
        }
    }
    test!(GetPalwModelRegistrationStatusResponse);

    impl Mock for GetPalwModelRequest {
        fn mock() -> Self {
            GetPalwModelRequest { class_id: mock_hex() }
        }
    }
    test!(GetPalwModelRequest);

    impl Mock for GetPalwModelResponse {
        fn mock() -> Self {
            GetPalwModelResponse {
                available: mock(),
                tip_daa: mock(),
                found: mock(),
                class_id: mock_hex(),
                model_name: "qwen".into(),
                n_ctx: mock(),
                artifact_root: mock_hex(),
                class_status: "Active".into(),
                registry_state: "Probation".into(),
                ready_seats: mock(),
                required_ready_seats: mock(),
                inflight_claims: mock(),
                admission_permille: mock(),
                share_permille: mock(),
                certified_family: mock_hex(),
                fence_active: mock(),
                reason: "probing".into(),
            }
        }
    }
    test!(GetPalwModelResponse);

    impl Mock for GetPalwModelReadinessRequest {
        fn mock() -> Self {
            GetPalwModelReadinessRequest { class_id: mock_hex() }
        }
    }
    test!(GetPalwModelReadinessRequest);

    impl Mock for RpcPalwModelSeatReadiness {
        fn mock() -> Self {
            RpcPalwModelSeatReadiness {
                seat_id: format!("{}:0", mock_hex()),
                bond_txid: mock_hex(),
                bond_index: mock(),
                proved_daa: mock(),
                proved_span: mock(),
                expires_daa: mock(),
                fresh: mock(),
                ready: mock(),
                collateral_sompi: mock(),
                needed_collateral_sompi: mock(),
                not_ready_reason: String::new(),
            }
        }
    }
    test!(RpcPalwModelSeatReadiness);

    impl Mock for GetPalwModelReadinessResponse {
        fn mock() -> Self {
            GetPalwModelReadinessResponse {
                available: mock(),
                tip_daa: mock(),
                found: mock(),
                class_id: mock_hex(),
                registry_state: "Prefetching".into(),
                ready_seats: mock(),
                required_ready_seats: mock(),
                seats: mock(),
            }
        }
    }
    test!(GetPalwModelReadinessResponse);

    impl Mock for GetPalwModelAdmissionRequest {
        fn mock() -> Self {
            GetPalwModelAdmissionRequest { class_id: mock_hex(), object_hex: "00".into() }
        }
    }
    test!(GetPalwModelAdmissionRequest);

    impl Mock for GetPalwModelAdmissionResponse {
        fn mock() -> Self {
            GetPalwModelAdmissionResponse {
                available: mock(),
                tip_daa: mock(),
                class_id: mock_hex(),
                admissible: mock(),
                processor_verdict: "ADMISSION_OK".into(),
                reject_code: String::new(),
                checks: mock(),
            }
        }
    }
    test!(GetPalwModelAdmissionResponse);

    impl Mock for GetPalwModelCertificationRequest {
        fn mock() -> Self {
            GetPalwModelCertificationRequest { class_id: mock_hex() }
        }
    }
    test!(GetPalwModelCertificationRequest);

    impl Mock for RpcPalwModelCertifiedFamily {
        fn mock() -> Self {
            RpcPalwModelCertifiedFamily { lane: "attempt".into(), digest: mock_hex(), covers: mock() }
        }
    }
    test!(RpcPalwModelCertifiedFamily);

    impl Mock for GetPalwModelCertificationResponse {
        fn mock() -> Self {
            GetPalwModelCertificationResponse {
                available: mock(),
                tip_daa: mock(),
                found: mock(),
                class_id: mock_hex(),
                end_to_end_certified: mock(),
                families: mock(),
            }
        }
    }
    test!(GetPalwModelCertificationResponse);

    impl Mock for RpcPalwModelLifecycle {
        fn mock() -> Self {
            RpcPalwModelLifecycle {
                class_id: mock_hex(),
                artifact_root: mock_hex(),
                is_base_class: mock(),
                has_row: mock(),
                state: "Active".to_string(),
                since_span: mock(),
                verification_ccu: "21070759296".to_string(),
                economic_ccu_per_claim: "18055200736".to_string(),
                artifact_bytes: mock(),
                ops_supported: mock(),
                verification_window_spans: mock(),
                artifact_prefetch_spans: mock(),
                max_inflight_claims: mock(),
                required_ready_seats: mock(),
                registration_bond_sompi: mock(),
                admission_claims_per_span_milli: mock(),
                probes_passed: mock(),
                probes_failed: mock(),
                ready_seats: mock(),
                inflight_claims: mock(),
                utilization_permille: mock(),
                admission_milli: mock(),
                cap_utilization_permille: mock(),
                priced_share_permille: mock(),
                work_ratio_permille: mock(),
                expected_forwards_q32: "2400000000000".to_string(),
                work_ticket_target: "2400000000000".to_string(),
                class_target: "2400000000000".to_string(),
                panel_room: mock(),
                final_work_share_10_permille: mock(),
                final_work_share_100_permille: mock(),
                ready_seats_now: mock(),
                inflight_now: mock(),
                share_permille: mock(),
                no_capable_panel_voids: mock(),
                reason: "admitting in full".to_string(),
            }
        }
    }

    test!(RpcPalwModelLifecycle);

    impl Mock for RpcPalwSeatReadiness {
        fn mock() -> Self {
            RpcPalwSeatReadiness {
                bond_txid: mock_hex(),
                bond_index: mock(),
                class_id: mock_hex(),
                proved_daa: mock(),
                proved_span: mock(),
                leaf_index: mock(),
                fresh: mock(),
                not_ready_reason: "stale".to_string(),
            }
        }
    }

    test!(RpcPalwSeatReadiness);

    impl Mock for GetPalwModelRegistryResponse {
        fn mock() -> Self {
            GetPalwModelRegistryResponse {
                available: mock(),
                tip_daa: mock(),
                scheduled: mock(),
                fence_daa: mock(),
                active: mock(),
                grace_until_daa: mock(),
                span_daa: mock(),
                reference_work_per_span: "2400000000000".to_string(),
                reference_bytes_per_span: mock(),
                seat_count: mock(),
                spare_seats: mock(),
                utilization_permille: mock(),
                probation_claims: mock(),
                stable_epochs: mock(),
                readiness_probe_max_age_spans: mock(),
                readiness_collateral_multiple: mock(),
                classes: mock(),
                readiness: mock(),
                classes_active: mock(),
                classes_active_limited: mock(),
                classes_probation: mock(),
                classes_prefetching: mock(),
                classes_registered: mock(),
                classes_held: mock(),
                bonds_active: mock(),
                bonds_with_headroom: mock(),
                work_target_shadow: mock(),
                work_target: "2400000000000".to_string(),
                work_floor: "2400000000000".to_string(),
                work_network_draws_q32: "2400000000000".to_string(),
                work_effective: "2400000000000".to_string(),
                work_epoch_index: mock(),
                work_closed_model_blocks: mock(),
                work_closed_expected_blocks: mock(),
                work_rate_sompi_per_giga: mock(),
                panel_inflight_replay: "2400000000000".to_string(),
                panel_horizon_spans: mock(),
                final_work_epochs: mock(),
            }
        }
    }

    test!(GetPalwModelRegistryResponse);

    struct Misalign;

    impl Mock for Misalign {
        fn mock() -> Self {
            Misalign
        }
    }

    impl Serializer for Misalign {
        fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
            store!(u32, &1, writer)?;
            store!(u32, &2, writer)?;
            store!(u32, &3, writer)?;
            Ok(())
        }
    }

    impl Deserializer for Misalign {
        fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
            let version: u32 = load!(u32, reader)?;
            assert_eq!(version, 1);
            Ok(Self)
        }
    }

    #[test]
    fn test_misalignment() {
        test::<Misalign>("Misalign");
    }
}
