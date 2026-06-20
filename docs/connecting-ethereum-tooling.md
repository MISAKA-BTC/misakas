# Connecting Ethereum tooling to MISAKA (Foundry / Hardhat / ethers / viem / MetaMask)

Status 2026‑06‑20. The MISAKA node exposes an Ethereum JSON‑RPC endpoint (the `kaspa-eth-rpc`
adapter) on `--evm-rpc-listen` (default `:8545`). Unmodified Ethereum tooling connects to it. See
`ethereum-rpc-compat-matrix.md` for per‑method status and `evm-differences-from-ethereum.md` for the
compat profile.

```
EVM chain id : 0x4D534B (5067595)
EVM spec     : Shanghai
Native unit  : 18 decimals, symbol MSK
RPC URL      : http://<node-host>:8545   (HTTP JSON-RPC; no WebSocket yet)
```

> `eth_sendRawTransaction` admits the tx to the receiving node's EVM mempool **and broadcasts it
> over P2P** to EVM‑relay peers (§14.2), so it reaches mining nodes and is included even if the node
> you're connected to does not mine. You can point your tooling at any synced MISAKA node.

## MetaMask — add a custom network

Settings → Networks → Add network → Add manually:

| Field | Value |
|---|---|
| Network name | MISAKA Testnet (EVM) |
| New RPC URL | `http://<node-host>:8545` |
| Chain ID | `5067595` |
| Currency symbol | `MSK` |
| Block explorer URL | (optional) |

Then balance display, send, and contract interaction work. MetaMask polls `eth_chainId`,
`net_version`, `eth_blockNumber`, `eth_getBalance`, `eth_gasPrice`, `eth_maxPriorityFeePerGas`,
`eth_feeHistory`, `eth_estimateGas`, `eth_call`, `eth_sendRawTransaction`, `eth_getTransactionReceipt`,
`eth_getTransactionByHash`, `eth_getBlockByNumber` — all implemented.

## Foundry (`cast` / `forge`)

`foundry.toml`:
```toml
[profile.default]
evm_version = "shanghai"
```

```bash
RPC=http://<node-host>:8545
cast chain-id   --rpc-url $RPC          # 5067595
cast block-number --rpc-url $RPC
cast balance 0x... --rpc-url $RPC
forge create src/Counter.sol:Counter --rpc-url $RPC --private-key $PK --broadcast
cast call $C "number()(uint256)" --rpc-url $RPC
cast send $C "setNumber(uint256)" 123 --rpc-url $RPC --private-key $PK
cast receipt $TX --rpc-url $RPC
cast logs --rpc-url $RPC --address $C   # eth_getLogs
```

EIP‑1559 (the default) uses `eth_feeHistory`, which the adapter implements. (`--legacy` also works
for legacy txs.)

## Hardhat

```ts
// hardhat.config.ts
import { HardhatUserConfig } from "hardhat/config";
const config: HardhatUserConfig = {
  solidity: { version: "0.8.24", settings: { evmVersion: "shanghai", optimizer: { enabled: true, runs: 200 } } },
  networks: { misaka: { url: "http://<node-host>:8545", chainId: 0x4d534b, accounts: [process.env.PRIVATE_KEY!] } },
};
export default config;
```

## ethers v6 / viem

```js
// ethers v6
import { JsonRpcProvider, Contract, Wallet } from "ethers";
const p = new JsonRpcProvider("http://<node-host>:8545");
await p.getBlockNumber(); await p.getBalance(addr); await p.getCode(c);
const counter = new Contract(c, abi, p); await counter.number();          // eth_call
const w = new Wallet(pk, p); await (await new Contract(c, abi, w).setNumber(7n)).wait();

// viem
import { createPublicClient, createWalletClient, http } from "viem";
const pub = createPublicClient({ transport: http("http://<node-host>:8545") });
await pub.getChainId(); await pub.readContract({ address: c, abi, functionName: "number" });
```

## Solidity rule

Compile with `evmVersion = "shanghai"` and a pinned compiler. Use OpenZeppelin / ERC‑20/721/1155
unmodified. Do **not** use `block.timestamp`/`blockhash`/`prevrandao` for randomness on a BlockDAG —
use commit‑reveal, a VRF, or an oracle (see `evm-differences-from-ethereum.md`).

## Verified (testnet, unmodified tooling)

- **Foundry**: `forge create` deployed `Counter`; `cast call number()` → 0 → `cast send setNumber(123)`
  (receipt status 1 + the `NumberSet` event) → `cast call number()` → 123; `cast logs` returned the event.
- **ethers v6 / viem 2.52**: `getChainId`/`getBlockNumber`/`getBalance`/`getCode` + `readContract`
  (`number()` → 123) + `getLogs` against the deployed contract — no patches.
