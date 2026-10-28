// PR-9.5e: `RpcHash` is the RPC block-hash alias; widened to `Hash64`
// per ADR-0008 (block identity). Note: header `utxo_commitment` is a
// 32-byte accumulator commitment and uses `kaspa_hashes::Hash` directly,
// not this alias.
pub type RpcHash = kaspa_consensus_core::BlockHash;
