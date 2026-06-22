# MISAKA EVM — differences from Ethereum (audit compat profile)

Status 2026‑06‑20. This pins the compatibility target so an audit covers the **MISAKA‑specific
diff**, not "all of Ethereum". The EVM lane is `revm`‑backed (ADR‑0020); the JSON‑RPC adapter is
`kaspa-eth-rpc`. See also `ethereum-rpc-compat-matrix.md` (per‑method status), `audit-scope-evm-rpc.md`
(scope), `third_party_manifest.toml` (upstream deps), `misaka-evm-design-v0.4.md`, `misaka-evm-wallet-profile-v1.md`.

## Compatibility profile (frozen for v0.4)

| Field | Value |
|---|---|
| EVM spec | **Shanghai** (`revm::SpecId::SHANGHAI`, compile‑time asserted in `kaspa-evm/src/lib.rs`) |
| Tx types | **Legacy / EIP‑2930 / EIP‑1559** (allowlist enforced at admission + execution decode) |
| Chain id | `0x4D534B` (5067595) — bound by EIP‑155; mandatory |
| Native unit | 18 decimals; 1e10 wei per L1 sompi (`EVM_NATIVE_SCALE`) |
| Base fee | EIP‑1559 base fee tracked per EVM block (`EvmExecutionHeader.base_fee_per_gas`) |
| Account model | standard Ethereum accounts (balance/nonce/code/storage), keccak‑MPT state root |
| Address derivation | standard secp256k1 → `keccak(pubkey)[‑20:]` (wallet profile `misaka-evm-hd-v1`, `m/44'/60'/0'/0/i`) |

## Supported (in scope for application code — should work unmodified)

- EVM Shanghai bytecode + the Legacy/EIP‑2930/EIP‑1559 transaction envelopes.
- `CREATE` / `CREATE2` address derivation (identical to Ethereum — verified e2e: a deploy's
  `contractAddress` = `CREATE(from, nonce)`).
- Standard contracts: ERC‑20 / ERC‑721 / ERC‑1155, OpenZeppelin (unmodified), and their events.
- Ethereum event‑log format + `eth_getLogs` filtering.
- EIP‑1559 fee market + EIP‑155 replay protection.
- The core JSON‑RPC subset (`ethereum-rpc-compat-matrix.md`): `eth_sendRawTransaction`, `eth_call`,
  `eth_estimateGas`, state reads, block/tx/receipt/log queries.

## NOT supported (initially) — out of scope, rejected or absent

- **Transaction types beyond Shanghai**: EIP‑4844 blob txs, EIP‑7702 set‑code — rejected at
  admission (the executor never runs them; they cannot enter a payload block).
- **Cancun+ opcodes/semantics** (TLOAD/TSTORE/MCOPY/BLOBHASH/beacon‑root, etc.) — not enabled
  (spec pinned to Shanghai). Solidity MUST set `evmVersion = "shanghai"`.
- **Ethereum consensus / networking**: PoS (Engine API, Beacon API), `geth`/`reth` consensus,
  miner, devp2p — MISAKA is a BlockDAG/UTXO chain with its own consensus + P2P; none of this is
  ported, and the JSON‑RPC `engine_*` namespace is absent.
- **Wallet/node management RPC**: `personal_*` (key unlock), `admin_*`, `debug_*`, `trace_*`,
  `txpool_*` — not exposed.
- **Filter subscriptions**: `eth_subscribe` (WebSocket) + the `eth_newFilter` family — not yet
  (HTTP `eth_getLogs` only).

## On‑chain‑randomness caveat (for application authors)

`block.timestamp`, `blockhash`, and `prevrandao` are NOT secure randomness on a BlockDAG (a miner
can influence/observe them, and `prevrandao` is not a beacon value here). Do not use them for
gacha/loot/lottery — use commit‑reveal, a VRF, an oracle, or server‑signed reveals. This is an
application‑design rule, not an EVM difference, but it is audit‑relevant for BCG/NFT contracts.

## MISAKA‑specific additions (NOT in Ethereum — first‑class audit targets)

- **Bridge**: UTXO→EVM deposit‑lock + producer‑applied deposit‑claim; EVM→UTXO `MISAKA_WITHDRAW`
  precompile (`0x…F002`) materializing a synthetic UTXO. See `audit-scope-evm-rpc.md` Scope B.
- **System precompiles**: `0x…F001/F002/F003` (deposit/withdraw/ML‑DSA‑verify) — non‑Ethereum.
- **Acceptance model**: EVM execution is the *acceptance* of merged blocks' payloads on the
  selected‑parent chain, not a single linear block — see `misaka-evm-design-v0.4.md` §6.

## Commitment roots in `eth_getBlockBy*` (audit M-02)

- **`transactionsRoot`** is a standard Ethereum keccak256 ordered (index-keyed) trie over the
  raw EIP-2718 transaction bytes — standard tooling can verify inclusion proofs against it.
- **`receiptsRoot`** is a **MISAKA-custom commitment**: the root over the canonical Borsh
  `EvmReceipt` encoding, **not** the standard typed-receipt RLP trie. It is the value the L1
  header commits to and re-execution checks, so it is correct and deterministic — but standard
  Ethereum *receipt-trie* proofs (light clients, receipt Merkle proofs) will **not** verify
  against it. Do not advertise standard receipt-proof compatibility. Per-receipt fields
  (`status`, `logs`, block-global `logIndex`, `logsBloom`) are standard and correct via
  `eth_getTransactionReceipt` / `eth_getLogs`.
- Moving `receiptsRoot` to the standard typed-receipt RLP trie is possible but is an
  activation-fenced **consensus change** (the root is committed), so it is deferred to a future
  fork/re-genesis rather than shipped as an RPC tweak.
