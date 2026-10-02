# MISAKA EVM — differences from Ethereum (audit compat profile)

Status 2026‑10‑02 (rechecked against the tree; current network testnet‑12). This pins the
compatibility target so an audit covers the **MISAKA‑specific diff**, not "all of Ethereum". The
EVM lane is `revm`‑backed (ADR‑0020); the JSON‑RPC adapter is `kaspa-eth-rpc`. This page is the
single home of the compat profile below. See also
[`ethereum-rpc-compat-matrix.md`](ethereum-rpc-compat-matrix.md) (per‑method RPC status),
[`connecting-ethereum-tooling.md`](connecting-ethereum-tooling.md) (tool setup),
[`archive/audit-scope-evm-rpc.md`](archive/audit-scope-evm-rpc.md) (audit scope, archived),
[`third_party_manifest.toml`](../third_party_manifest.toml) (upstream deps),
[`design/misaka-evm-design-v0.4.md`](design/misaka-evm-design-v0.4.md),
[`misaka-evm-wallet-profile-v1.md`](misaka-evm-wallet-profile-v1.md).

## Compatibility profile

| Field | Value |
|---|---|
| EVM spec | **Shanghai** (`EVM_SPEC_ID = SpecId::SHANGHAI`, compile‑time asserted in `kaspa-evm/src/lib.rs`; also checked by `scripts/pq-ci-guard.sh`) |
| Tx types | **Legacy / EIP‑2930 / EIP‑1559** (allowlist enforced at admission + execution decode) |
| Chain id | `0x4D534B` (5067595, `EVM_CHAIN_ID` in `consensus/core/src/evm/mod.rs`) — bound by EIP‑155; mandatory. The code notes the mainnet id will be a different value chosen at mainnet launch |
| Native unit | 18 decimals, symbol MSK; 1e10 wei per L1 sompi (`EVM_NATIVE_SCALE`) |
| Base fee | EIP‑1559 base fee tracked per EVM block (`EvmExecutionHeader.base_fee_per_gas`); no priority‑fee market (tip is `0x0`) |
| Account model | standard Ethereum accounts (balance/nonce/code/storage), keccak‑MPT state root |
| Address derivation | standard secp256k1 → `keccak(pubkey)[‑20:]` (wallet profile `misaka-evm-hd-v1`, `m/44'/60'/0'/0/i`) |

## Supported (in scope for application code — should work unmodified)

- EVM Shanghai bytecode + the Legacy/EIP‑2930/EIP‑1559 transaction envelopes.
- `CREATE` / `CREATE2` address derivation (identical to Ethereum — a deploy's
  `contractAddress` = `CREATE(from, nonce)`).
- Standard contracts: ERC‑20 / ERC‑721 / ERC‑1155, OpenZeppelin (unmodified), and their events.
- Ethereum event‑log format + `eth_getLogs` filtering.
- EIP‑1559 fee market + EIP‑155 replay protection.
- The JSON‑RPC subset listed in [`ethereum-rpc-compat-matrix.md`](ethereum-rpc-compat-matrix.md)
  (HTTP + WebSocket `eth_subscribe`).

## NOT supported — out of scope, rejected or absent

- **Transaction types beyond Shanghai**: EIP‑4844 blob txs, EIP‑7702 set‑code — rejected at
  admission (the executor never runs them; they cannot enter a payload block).
- **Cancun+ opcodes/semantics** (TLOAD/TSTORE/MCOPY/BLOBHASH/beacon‑root, etc.) — not enabled
  (spec pinned to Shanghai). Solidity MUST set `evmVersion = "shanghai"`.
- **Ethereum consensus / networking**: PoS (Engine API, Beacon API), `geth`/`reth` consensus,
  miner, devp2p — MISAKA is a BlockDAG/UTXO chain with its own consensus + P2P; none of this is
  ported, and the JSON‑RPC `engine_*` namespace is absent.
- **RPC methods/namespaces not implemented** (filters, `eth_getProof`, `personal_*`, `admin_*`,
  `txpool_*`, most of `debug_*`/`trace_*`): the authoritative list is the "Not implemented"
  section of [`ethereum-rpc-compat-matrix.md`](ethereum-rpc-compat-matrix.md).

## On‑chain‑randomness caveat (for application authors)

`block.timestamp`, `blockhash`, and `prevrandao` are NOT secure randomness on a BlockDAG (a miner
can influence/observe them, and `prevrandao` is not a beacon value here). Do not use them for
gacha/loot/lottery — use commit‑reveal, a VRF, an oracle, or server‑signed reveals. This is an
application‑design rule, not an EVM difference, but it is audit‑relevant for BCG/NFT contracts.

## MISAKA‑specific additions (NOT in Ethereum — first‑class audit targets)

- **Bridge**: UTXO→EVM deposit‑lock + producer‑applied deposit‑claim; EVM→UTXO `MISAKA_WITHDRAW`
  precompile (`0x…F002`) materializing a synthetic UTXO. See
  [`archive/audit-scope-evm-rpc.md`](archive/audit-scope-evm-rpc.md) Scope B.
- **Reserved system addresses** (`consensus/core/src/evm/mod.rs`):
  - `0x…F001` — `WMISAKA_ADDRESS`, the WETH9‑equivalent wrapped‑native ERC‑20. A normal contract
    predeployed into the activation state, **not** a precompile.
  - `0x…F002` — `MISAKA_WITHDRAW_PRECOMPILE` (EVM → UTXO withdraw).
  - `0x…F003` — `MISAKA_MLDSA_VERIFY_PRECOMPILE` (ML‑DSA‑87 verify; pure, no state change).
    Activation‑fenced by `evm_f003_mldsa_verify_activation_daa_score`; testnet‑12 arms it at DAA 0
    (`palw_t12_arm_every_rule_from_genesis` in `consensus/core/src/config/params.rs`).
- **Acceptance model**: EVM execution is the *acceptance* of merged blocks' payloads on the
  selected‑parent chain, not a single linear block — see
  [`design/misaka-evm-design-v0.4.md`](design/misaka-evm-design-v0.4.md) §6.

## Commitment roots in `eth_getBlockBy*` (audit M-02)

- **`transactionsRoot`** is a standard Ethereum keccak256 ordered (index-keyed) trie over the
  raw EIP-2718 transaction bytes — standard tooling can verify inclusion proofs against it.
- **`receiptsRoot`** depends on the `evm_typed_receipt_root_activation_daa_score` fence
  (`kaspa-evm/src/roots.rs`):
  - **at/above the fence (`receipts_root_v2`)** — the exact Ethereum EIP-2718 *typed* receipt
    root (legacy receipt = `rlp([status, cumulativeGasUsed, logsBloom, logs])`, typed receipt =
    `type || rlp(...)`, keccak MPT keyed by `rlp(index)`). **testnet-12 arms this fence at DAA 0**,
    so every testnet-12 block carries the standard receipt root.
  - **below the fence (`receipts_root`, v1)** — a MISAKA-custom root over the Borsh `EvmReceipt`
    encoding; standard receipt-trie proofs do **not** verify against it. The static presets that
    leave the fence dormant (`u64::MAX`) stay on v1.
- Per-receipt fields (`status`, `logs`, block-global `logIndex`, `logsBloom`) are standard via
  `eth_getTransactionReceipt` / `eth_getLogs` under either root.
