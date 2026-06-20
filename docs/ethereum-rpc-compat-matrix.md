# Ethereum JSON-RPC compatibility matrix (MISAKA eth-rpc adapter)

Status 2026‑06‑20. The adapter is the `kaspa-eth-rpc` crate (`rpc/eth`) served by kaspad on
`--evm-rpc-listen` (default `:8545`), HTTP JSON‑RPC 2.0 (+ batch + CORS). It is a thin front end
over the node‑side `EthProvider` (`kaspad/src/eth_rpc.rs`); all consensus reads + the read‑only
revm simulation live node‑side. EVM spec = **Shanghai**, `EVM_CHAIN_ID = 0x4D534B` (5067595),
native = 18 decimals (1e10 wei/sompi). Tx types: **Legacy / EIP‑2930 / EIP‑1559** only.

Legend: **✓** full · **◑** works with a documented limitation · **·** stubbed constant · **✗** not implemented (returns JSON‑RPC `-32601`).

| Method | Status | Notes |
|---|:--:|---|
| `web3_clientVersion` | ✓ | `misaka-kaspad/v<ver>` |
| `web3_sha3` | ✓ | keccak256 |
| `net_version` | ✓ | chain id as a decimal string (`5067595`) |
| `net_listening` | · | constant `true` |
| `net_peerCount` | · | constant `0x0` (P2P peer count not surfaced) |
| `eth_chainId` | ✓ | `0x4d534b` |
| `eth_blockNumber` | ✓ | canonical EVM head number (sink) |
| `eth_syncing` | ◑ | always `false` (the endpoint serves once the node is up) |
| `eth_gasPrice` | ◑ | fixed 1 gwei suggestion (not the live base fee yet) |
| `eth_maxPriorityFeePerGas` | · | `0x0` (no priority‑fee market) |
| `eth_accounts` | ✓ | `[]` (the node holds no keys — correct) |
| `eth_getBalance` | ✓ | at the latest head; historical block tags pending the index backfill |
| `eth_getTransactionCount` | ✓ | nonce at the latest head |
| `eth_getCode` | ✓ | at the latest head |
| `eth_getStorageAt` | ✓ | at the latest head |
| `eth_call` | ◑ | revm read‑only at the **head** (no historical block tag yet) |
| `eth_estimateGas` | ✓ | binary search over revm simulation |
| `eth_sendRawTransaction` | ✓ | admits to the EVM mempool **and P2P‑broadcasts** to relay peers, so it mines even if the node you connected to does not mine |
| `eth_getTransactionReceipt` | ✓ | status / from / to / contractAddress / type / effectiveGasPrice / gasUsed / cumulativeGasUsed / logs / blockHash / blockNumber. `logsBloom` is zero‑filled (not recomputed per‑receipt) |
| `eth_getTransactionByHash` | ◑ | full decoded tx + block context; `v/r/s` are `0x0` (not surfaced) |
| `eth_getBlockByNumber` | ◑ | tags (`latest`/`safe`/`finalized`/`earliest`/`pending`) + hex N; `transactions` are **hashes** (full‑tx objects pending); `parentHash` is zero |
| `eth_getBlockByHash` | ◑ | same as by‑number; the 32‑byte block id = first 32 bytes of the 64‑byte L1 hash |
| `eth_getBlockTransactionCountByNumber` | ✓ | |
| `eth_getBlockTransactionCountByHash` | ✓ | |
| `eth_getLogs` | ◑ | `address` + `topics` (OR/wildcard) + `blockHash`/`fromBlock`/`toBlock`; **10 000‑block range cap** + 10 000‑result cap; **forward‑populated index** (blocks committed after the node ran this binary; a historical backfill is a follow‑up) |
| `eth_feeHistory` | ◑ | real base fees + `gasUsedRatio` over the range (+1 projection); reward percentiles are `0x0` (no priority‑fee market). Enables default EIP‑1559 tooling (Foundry/ethers/viem/MetaMask) |

**Not implemented** (return `-32601`, by design for the MVP): `eth_subscribe`/`eth_unsubscribe`
(WebSocket), `eth_newFilter`/`eth_getFilterChanges`/`eth_uninstallFilter`,
`eth_getProof`, `eth_getBlockReceipts`, `eth_getTransactionByBlock*AndIndex`, and all
`personal_*` / `admin_*` / `engine_*` / `debug_*` / `trace_*` / `txpool_*` namespaces.

### Verified live (testnet, 2026‑06‑20)
- Identity + state: `eth_chainId 0x4d534b`, `eth_getBalance` returned a bridge‑credited
  `0xde0b6b3a7640000` (1 MSK = 1e18 wei), `eth_estimateGas` = `0x5208` (21000 intrinsic).
- Block/log index: `eth_getBlockByNumber("latest")` / by‑N / by‑hash resolve the same canonical
  block; `eth_getLogs` range‑cap returns `-32000`.
- Full contract deploy: `eth_sendRawTransaction` deployed a CREATE tx whose constructor emits
  `LOG0` → `eth_getTransactionReceipt` returned `status 0x1` + `contractAddress` + `from` + the
  log; `eth_getLogs({address})` returned the event; `eth_getTransactionByHash` returned the tx.
