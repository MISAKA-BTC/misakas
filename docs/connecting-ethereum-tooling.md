# Connecting Ethereum tooling to MISAKA (Foundry / Hardhat / ethers / viem / MetaMask)

Status 2026‑10‑02 (current network testnet‑12). The MISAKA node exposes an Ethereum JSON‑RPC
endpoint (the `kaspa-eth-rpc` adapter). Unmodified Ethereum tooling connects to it. See
[`ethereum-rpc-compat-matrix.md`](ethereum-rpc-compat-matrix.md) for per‑method status and
[`evm-differences-from-ethereum.md`](evm-differences-from-ethereum.md) for the compat profile
(chain id, EVM spec, tx types, units).

```
EVM chain id : 0x4D534B (5067595)
EVM spec     : Shanghai
Native unit  : 18 decimals, symbol MSK
RPC URL      : http://<node-host>:8545   (HTTP JSON-RPC)
WebSocket    : ws://<node-host>:8545      (same listener; eth_subscribe)
```

## Enabling the endpoint on a node

The adapter is **off** unless a listener is configured (`kaspad/src/args.rs`):

- `--evm-rpc-listen=127.0.0.1:8545` (alias `--evm-rpc-http-listen`, env `KASPAD_EVM_RPC_LISTEN`;
  default port 8545), or a profile that sets it: `--profile=local-full` (loopback) /
  `--profile=public-evm-rpc` (`0.0.0.0`). An explicit `--evm-rpc-listen` overrides the profile.
- The adapter is unauthenticated and CORS‑open, so a **non‑loopback bind refuses to start** unless
  `MISAKA_ALLOW_PUBLIC_EVM_RPC=1` is set (`kaspad/src/daemon.rs`). Prefer a loopback bind behind a
  TLS + auth + rate‑limiting reverse proxy.
- The sync‑only `--node-profile`s (`bootstrap-pruned`, `recovery-sync`) reject
  `--evm-rpc-listen`.

`eth_sendRawTransaction` also P2P‑broadcasts to EVM‑relay peers, so you can point your tooling at
any synced MISAKA node, mining or not (see the matrix).

## MetaMask — add a custom network

Settings → Networks → Add network → Add manually:

| Field | Value |
|---|---|
| Network name | MISAKA Testnet (EVM) |
| New RPC URL | `http://<node-host>:8545` |
| Chain ID | `5067595` |
| Currency symbol | `MSK` |
| Block explorer URL | (optional) |

Then balance display, send, and contract interaction work; every method MetaMask polls is in the
compat matrix.

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

Compile with `evmVersion = "shanghai"` and a pinned compiler; OpenZeppelin / ERC‑20/721/1155 work
unmodified. Read the on‑chain‑randomness caveat in
[`evm-differences-from-ethereum.md`](evm-differences-from-ethereum.md) before using block fields.
