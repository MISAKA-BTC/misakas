# Audit scope — MISAKA EVM lane + Ethereum JSON-RPC adapter + bridge + indexer

Status 2026‑06‑20. Purpose: bound the audit to the **MISAKA diff** (adapter + bridge + indexer +
the revm integration) and keep unmodified upstream (revm/alloy, OpenZeppelin) to a dependency
review. Read with `evm-differences-from-ethereum.md`, `ethereum-rpc-compat-matrix.md`,
`third_party_manifest.toml`.

## What is upstream‑unchanged (NOT audit‑custom — dependency review only)

| Component | Crate / version | License | Modified? |
|---|---|---|---|
| EVM interpreter + state transition | `revm` 14 (default‑features off, `std`) | MIT | **No** — used as a library |
| Ethereum primitive types / RLP / ABI | `alloy-primitives` 0.8, `alloy-rlp` 0.3, `alloy-trie` 0.7 | Apache‑2.0 / MIT | **No** |
| Tx envelope decode / signer recovery | `alloy-consensus` / `alloy-eips` (0.8 line) | Apache‑2.0 / MIT | **No** |
| RPC response shapes | `alloy-rpc-types-eth` 0.8 | Apache‑2.0 / MIT | **No** |
| Application contracts | OpenZeppelin Contracts | MIT | **Must stay unmodified** (else it re‑enters scope) |

Auditing these = reviewing the *version pin + how MISAKA calls them*, not the upstream code.

## Audit‑custom surface (≈3.9 kLOC of MISAKA‑specific EVM code + the indexes)

### Scope A — EVM consensus execution (`kaspa-evm`)
The revm integration that turns merged‑block payloads into committed EVM state: the executor
(`executor.rs`/`state.rs`/`env.rs`/`snapshot.rs`/`roots.rs`), tx admission + canonical 2718 decode
+ signer recovery (`tx.rs`), the Shanghai pin, and the read‑only `eth_call`/`estimateGas`
simulation (`sim.rs`). **Audit focus**: determinism (construction == validation), the canonical
‑encoding/hash‑malleability gate, gas/fee accounting, the supply invariant.

### Scope B — Bridge (UTXO ⇄ EVM)
`consensus/src/processes/evm/` + `crypto/txscript` deposit‑lock script + `kaspa-pq-validator`
deposit‑lock/claim. **Audit focus**: the deposit‑lock → claim credit path (no double‑credit; lock
consumed once on the canonical chain), the `MISAKA_WITHDRAW` `0x…F002` precompile + the
**domain‑separated synthetic withdrawal outpoint** (`synthetic_withdrawal_txid`), refund‑timeout
semantics, the EIP‑55 deposit‑address guard. PR‑1 (synthetic‑outpoint safety) already shipped.

### Scope C — Ethereum JSON‑RPC adapter (`rpc/eth` + `kaspad/src/eth_rpc.rs`)
The hand‑rolled HTTP/1.1 JSON‑RPC 2.0 server (no axum/hyper), the `EthProvider` node‑side impl,
quantity/type encoding, and method dispatch. **Audit focus**:
- **No consensus mutation** — every method is a read or a mempool submit; `eth_call`/`estimateGas`
  run revm *without commit*.
- **Correctness of encodings** (QUANTITY/DATA, 32‑byte ids), so unmodified tooling behaves.
- **DoS surface**: the 4 MiB body cap, the `eth_getLogs` 10 000‑block + 10 000‑result caps, the
  per‑connection `Connection: close`, CORS `*`. Confirm there is no unbounded scan.
- **secp‑free default**: the crate links no revm/secp; the provider is `#[cfg(feature = "evm")]`
  only, so the default node keeps its secp‑free guarantee (`scripts/pq-ci-guard.sh`).

### Scope D — Application contracts
Upstream OpenZeppelin/ERC standards = dependency review only. **Audit**: the custom BCG/marketplace
contracts, and any randomness/oracle/fee‑split logic. Keep `evmVersion = "shanghai"` + a pinned
compiler; isolate any chain‑specific precompile dependency behind an adapter contract.

### Scope E — Indexer (eth‑rpc read indexes)
The EVM DB stores (`database/src/registry.rs`, prefixes 201–213, excl. 212): header / state /
receipts / tx‑lookup / logs / payload / canonical‑heads + the eth‑rpc indexes `EvmBlockHashMap`
(210) + `EvmNumberIndex` (213). **Audit focus**:
- **RPC‑only, never committed** — index rows are not in any commitment/validation, so other nodes
  without the index code do not split.
- **Reorg correctness**: the number/hash indexes are upsert; readers re‑validate
  `is_chain_block(hash) && header.evm_number == n` so a reorg‑orphaned row reads as absent (the
  `get_evm_tx_receipt` canonical‑resolution pattern). Confirm no stale/duplicate/missing log.
- Known limitations to note (not vulns): forward‑only population (historical backfill is a
  follow‑up), `parentHash`=0, full‑tx blocks + `v/r/s` not surfaced.

## Known open items (disclose to auditors; tracked as follow‑ups)
- No WebSocket subscriptions / filter objects; `eth_getLogs` (HTTP) only.
- `eth_getLogs` index is forward‑populated (historical backfill is a follow‑up); receipt `logsBloom`
  is zero‑filled; block `parentHash`/full‑tx objects + tx `v/r/s` are not surfaced.

(Resolved: `eth_sendRawTransaction` now P2P‑broadcasts via `flow_context.submit_rpc_evm_transaction`,
so a tx submitted to a non‑mining node relays to mining peers — the adapter no longer requires a
mining node.)

## Out of scope
Upstream Ethereum consensus/networking (none ported), `personal_*`/`admin_*`/`engine_*` (absent),
and the PQ/UTXO/PoW/DNS‑finality layers (covered by their own ADR audits, not the EVM diff).
