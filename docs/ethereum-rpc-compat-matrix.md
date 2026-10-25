# Ethereum JSON-RPC compatibility matrix (MISAKA eth-rpc adapter)

> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

Status 2026‑06‑27. The adapter is the `kaspa-eth-rpc` crate (`rpc/eth`) served by kaspad on
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
| `eth_syncing` | ◑ | a derived boolean: `true` while the node is still catching up (not `is_nearly_synced`), `false` once nearly synced. The full Ethereum progress object (`{startingBlock,currentBlock,highestBlock}`) is not surfaced — a truthy value just signals "not ready" |
| `eth_gasPrice` | ✓ | the live head EIP‑1559 base fee (falls back to the genesis initial base fee if the head header is briefly unavailable) |
| `eth_maxPriorityFeePerGas` | · | `0x0` (no priority‑fee market) |
| `eth_accounts` | ✓ | `[]` (the node holds no keys — correct) |
| `eth_getBalance` | ✓ | honors the block selector (audit H‑04): `latest`/`pending`, `safe`/`finalized` (non‑reorgable heads), `earliest`, a hex number, and EIP‑1898 by‑hash (with `requireCanonical`). A historical block reads that block's snapshot (hot, else §12 reconstruction) and fails **closed** if unavailable |
| `eth_getTransactionCount` | ✓ | nonce at the block selector (same resolution as `eth_getBalance`); `pending` adds this node's contiguous mempool nonces (audit M‑08) |
| `eth_getCode` | ✓ | at the block selector (same resolution as `eth_getBalance`) |
| `eth_getStorageAt` | ✓ | at the block selector (same resolution as `eth_getBalance`); returns the full 32‑byte DATA value |
| `eth_call` | ✓ | revm read‑only; honors the block param (`latest`/`pending`, `earliest`, `safe`/`finalized`, a hex number, and EIP‑1898 `{blockNumber}`/`{blockHash,requireCanonical}`). A historical block rebuilds that block's state (hot snapshot, else checkpoint/diff reconstruction) and uses that block's env (number/timestamp/coinbase/gas limit/chain id); fails **closed** if the state is unavailable (pruned / pre‑activation) |
| `eth_estimateGas` | ✓ | binary search over revm simulation; honors the same block param as `eth_call` |
| `eth_sendRawTransaction` | ✓ | admits to the EVM mempool **and P2P‑broadcasts** to relay peers, so it mines even if the node you connected to does not mine |
| `eth_getTransactionReceipt` | ✓ | status / from / to / contractAddress / type / effectiveGasPrice / gasUsed / cumulativeGasUsed / logs / blockHash / blockNumber. `logsBloom` is the real EIP‑234 bloom over the receipt's logs and each log's `logIndex` is block‑global (audit H‑05) |
| `eth_getTransactionByHash` | ✓ | full decoded tx + block context; real `v`/`r`/`s` signature (audit R‑3); typed (EIP‑2930/1559) txs also surface `yParity` + `accessList` |
| `eth_getBlockByNumber` | ◑ | tags (`latest`/`safe`/`finalized`/`earliest`/`pending`) + hex N; `transactions` are **hashes** by default and full‑tx objects when the `fullTransactionObjects` flag is `true` (audit H‑04); `parentHash` is the `evm_number − 1` block's id |
| `eth_getBlockByHash` | ◑ | same as by‑number; the 32‑byte block id = first 32 bytes of the 64‑byte L1 hash |
| `eth_getBlockTransactionCountByNumber` | ✓ | |
| `eth_getBlockTransactionCountByHash` | ✓ | |
| `eth_getLogs` | ◑ | `address` + `topics` (OR/wildcard) + `blockHash`/`fromBlock`/`toBlock`; **10 000‑block range cap** + 10 000‑result cap; **forward‑populated index** (blocks committed after the node ran this binary; a historical backfill is a follow‑up) |
| `eth_feeHistory` | ◑ | real base fees + `gasUsedRatio` over the range (+1 projection); reward percentiles are `0x0` (no priority‑fee market). Enables default EIP‑1559 tooling (Foundry/ethers/viem/MetaMask) |
| `debug_traceTransaction` | ◑ | re‑executes an **accepted** tx against its exact pre‑state. Tracer selector (param #2 `{tracer}`): omit ⇒ the Geth default opcode/struct logger (memory/storage omitted by §11.5); `callTracer` ⇒ the call‑frame tree (with `misakaOriginatingPayloadBlock`/`misakaAcceptingBlock` on the root); `prestateTracer` ⇒ the diffMode pre/post state. `null` for an unknown / not‑accepted (skipped/pending) tx — use `misaka_getEvmTxStatus`/`misaka_traceEvmCandidate` to diagnose those. Any other tracer name ⇒ `-32602` |
| `trace_transaction` | ◑ | the Parity/OpenEthereum flat‑call list (`[{action,result\|error,subtraces,traceAddress,type}]`) of the SAME accepted‑tx replay as `debug_traceTransaction`'s `callTracer`. `null` for a non‑accepted tx |
| `misaka_traceEvmCandidate` | ✓ | MISAKA extension (§11.6): diagnoses a tx with **no receipt** (skipped class 2/3/5 or still pending) by replaying it against the current head — returns `executed`/`accepted`/`status`/`gasUsed`/`reason`/`recordedSkipClass` + the call tree. `null` if the raw tx is unknown to the node |
| `misaka_getEvmTxStatus` | ✓ | MISAKA extension: the full EVM‑lane lifecycle of a tx (`pending`/`included`/`accepted`/`skipped`/`unknown`) — strictly more than `eth_getTransactionReceipt`'s accepted‑or‑null. `acceptedIn` is reported ONLY for a canonical (non‑reorgable) acceptance (audit H‑06) |

> The `debug_*`/`trace_*`/`misaka_*` trace methods require an `--features evm` node (the
> read‑only revm executor). A non‑EVM node returns `-32601` for them.

**WebSocket** (`ws://<node-host>:8545`, same listener): ordinary JSON‑RPC requests work
over the socket, plus `eth_subscribe`/`eth_unsubscribe` for the subscription kinds
`newHeads`, `newPendingTransactions`, and `logs` (the `logs` filter takes the same
`{address,topics}` shape as `eth_getLogs`; an **unfiltered** all‑logs subscription is
refused — supply at least one address or a non‑wildcard topic). Caps: 64 subscriptions
per connection; a slow consumer (bounded outbound queue) is disconnected.

**Not implemented** (return `-32601`, by design for the MVP):
`eth_newFilter`/`eth_getFilterChanges`/`eth_uninstallFilter`,
`eth_getProof`, `eth_getBlockReceipts`, `eth_getTransactionByBlock*AndIndex`, and the
`personal_*` / `admin_*` / `engine_*` / `txpool_*` namespaces (and any `debug_*` /
`trace_*` method other than the rows above).

### Verified live (testnet, 2026‑06‑20)
- Identity + state: `eth_chainId 0x4d534b`, `eth_getBalance` returned a bridge‑credited
  `0xde0b6b3a7640000` (1 BILI = 1e18 wei), `eth_estimateGas` = `0x5208` (21000 intrinsic).
- Block/log index: `eth_getBlockByNumber("latest")` / by‑N / by‑hash resolve the same canonical
  block; `eth_getLogs` range‑cap returns `-32000`.
- Full contract deploy: `eth_sendRawTransaction` deployed a CREATE tx whose constructor emits
  `LOG0` → `eth_getTransactionReceipt` returned `status 0x1` + `contractAddress` + `from` + the
  log; `eth_getLogs({address})` returned the event; `eth_getTransactionByHash` returned the tx.
