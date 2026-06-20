# MISAKA EVM examples — a minimal BCG contract set

A self-contained ERC-20 / ERC-721 / Marketplace set you can deploy to MISAKA with **unmodified
Foundry / Hardhat / ethers / viem**, demonstrating the in-game token + NFT + fixed-price
marketplace pattern. The contracts have **no external imports** so `forge create` works without
`forge install`. For production, swap them for audited **OpenZeppelin** equivalents
(`ERC20`/`ERC721`/`AccessControl`/`Ownable`/`Pausable`/`ERC2981`).

| Contract | Role |
|---|---|
| `contracts/GameToken.sol` | ERC-20 in-game fungible token (`mint`, `transfer`, `approve`, `transferFrom`) |
| `contracts/GameItem.sol` | ERC-721 item / character / skin (`mint`, `transferFrom`, `approve`, `setApprovalForAll`) |
| `contracts/Marketplace.sol` | fixed-price NFT marketplace (`list` / `cancel` / `buy`), pays the seller in native MSK |

Compiled with `evmVersion = "shanghai"` (see `foundry.toml`). MISAKA EVM chain id = `0x4D534B`
(5067595). See `../../docs/connecting-ethereum-tooling.md` for the RPC setup (MetaMask/Hardhat/ethers/viem)
and `../../docs/evm-differences-from-ethereum.md` for the compat profile.

## Deploy + interact (Foundry)

```bash
export RPC=http://<node-host>:8545      # any synced MISAKA node (txs relay to miners)
export PK=0x<your-private-key>
cd examples/evm

# ERC-20
TOKEN=$(forge create contracts/GameToken.sol:GameToken --rpc-url $RPC --private-key $PK --broadcast \
        | sed -n 's/Deployed to: //p')
cast send $TOKEN "mint(address,uint256)" $MY_ADDR 1000000000000000000000 --rpc-url $RPC --private-key $PK
cast call $TOKEN "balanceOf(address)(uint256)" $MY_ADDR --rpc-url $RPC          # 1000e18

# ERC-721
ITEM=$(forge create contracts/GameItem.sol:GameItem --rpc-url $RPC --private-key $PK --broadcast \
       | sed -n 's/Deployed to: //p')
cast send $ITEM "mint(address)" $MY_ADDR --rpc-url $RPC --private-key $PK         # mints id 1
cast call $ITEM "ownerOf(uint256)(address)" 1 --rpc-url $RPC

# Marketplace — NOTE: pass --constructor-args LAST (Foundry's variadic flag is greedy)
MKT=$(forge create contracts/Marketplace.sol:Marketplace --rpc-url $RPC --private-key $PK --broadcast \
      --constructor-args $ITEM | sed -n 's/Deployed to: //p')
cast send $ITEM "approve(address,uint256)" $MKT 1 --rpc-url $RPC --private-key $PK
cast send $MKT "list(uint256,uint256)" 1 1000000000000000 --rpc-url $RPC --private-key $PK   # 0.001 MSK

# buy (run from a SECOND funded account = the buyer):
cast send $MKT "buy(uint256)" 1 --value 1000000000000000 --rpc-url $RPC --private-key $BUYER_PK
```

Fund an EVM address by bridging from the UTXO side:
`kaspa-pq-validator deposit-lock --evm-address 0x… --amount <sompi>` → `… claim --outpoint <txid>:0`
(the claim credits `amount` sompi × 1e10 wei). See `../../docs/misaka-evm-wallet-profile-v1.md`.

## Hardhat / ethers / viem

Same contracts; point the network at `http://<node-host>:8545`, `chainId: 0x4d534b`,
`evmVersion: "shanghai"`. Snippets in `../../docs/connecting-ethereum-tooling.md`.

## Verified live (testnet, 2026-06-20, unmodified Foundry 1.7.1)

- **GameToken**: `mint(A, 1000e18)` → `balanceOf(A)` = `1000e18`; `transfer(B, 25e18)` → `balanceOf(B)` = `25e18`.
- **GameItem**: `mint(A)` → `ownerOf(1)` = `A`.
- **Marketplace**: `approve` + `list(1, 0.001 MSK)` → `listings(1)` = `[A, 1e15]` + a `Listed` event
  (read via `eth_getLogs` / `cast logs`). Cross-contract reads (`ownerOf`/`getApproved`) + approval execute correctly.

(Example addresses from that run — testnet, will differ on each deploy: GameToken
`0x59AF421cB35fc23aB6C8ee42743e6176040031f4`, GameItem `0x47eb28D8139A188C5686EedE1E9D8EDE3Afdd543`,
Marketplace `0x7f1C87Bd3a22159b8a2E5D195B1a3283D10ea895`.)

## Randomness warning (BCG-relevant)

Do **not** use `block.timestamp` / `blockhash` / `prevrandao` for gacha / loot / lottery on a
BlockDAG — use commit-reveal, a VRF, an oracle, or server-signed reveals. See
`../../docs/evm-differences-from-ethereum.md`.
