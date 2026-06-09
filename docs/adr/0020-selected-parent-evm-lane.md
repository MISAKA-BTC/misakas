# ADR-0020: Selected-Parent EVM Execution Lane on L1

## Status
Proposed (2026-06-10). **P0 (spec freeze) + P1 (consensus types) implemented**; P2–P7 pending.

Source design: `MISAKA_Kaspa_L1_Selected_Parent_EVM_Design.docx` (Draft v0.1). This ADR is the
code-grounded freeze of that design against the current kaspa-pq tree.

Adds header fields and a block-body payload → a hard fork, but **version-gated** so every existing
v0/v1 genesis hash and block identity is byte-for-byte unchanged (see §"Version gating"). Interacts
with the PQ-only invariant ([ADR-0019](0019-mldsa87-migration.md)): the EVM lane is a **separate
signature domain** that reintroduces secp256k1/ECDSA, isolated behind the `evm` cargo feature so the
default node build stays secp-free.

---

## Context

We want to run the Ethereum EVM as part of L1 consensus — no external bridge, no L2 sequencer —
while keeping the Kaspa/MISAKA DAG consensus and UTXO ledger intact. The core tension is that the
EVM is a **global mutable state machine** (inherently sequential), whereas a DAG accepts many blocks
in parallel and reorganizes its virtual selected chain frequently. Running every blue/accepted
block's EVM txs would force a re-ordering/replay problem on every virtual change.

## Decision — Selected-Parent EVM Lane

The EVM parent of a DAG block `B` is its GHOSTDAG **`selected_parent(B)`** — not its full direct-parent
set, and not the current virtual selected parent:

```
EVM_PARENT(B) = selected_parent(B)
EVM_STATE(B)  = EXEC(EVM_STATE(EVM_PARENT(B)), system_deposits(B), evm_txs(B), env(B))
```

Consequences of this single rule:
- `B`'s EVM result is an **append-only function of `B` alone** — computed once at block validation,
  stored by `block_hash`, and **never re-executed** on a virtual reorg.
- A virtual change only moves a **canonical EVM head pointer** (`latest_unsafe` / `safe` / `finalized`);
  no `execute_evm` / `revert_evm` on the hot path.
- EVM txs are canonical only when their block enters the selected-parent chain; UTXO txs keep their
  existing DAG-inclusive acceptance. This asymmetry is intentional (design §3.3).
- UTXO ↔ EVM value moves via in-consensus **system deposit / withdraw** side-effects, conserving the
  combined native-coin supply (design §6/§7).
- RPC separates `latest` / `safe` / `finalized` heads; USDC/CEX-grade use targets `finalized`.

Trade-off (accepted): EVM throughput tracks the single selected-parent chain, **not** DAG parallelism.

---

## Frozen parameters (P0)

| Item | Value | Notes |
|---|---|---|
| `EVM_HEADER_VERSION` | `2` | `constants.rs`. Must exceed genesis v0 and `BLOCK_VERSION`=1. Never lower. |
| `EVM_CHAIN_ID` | `0x4D534B` ("MSK") | `evm/mod.rs`. Distinct from all public Ethereum nets; mainnet id chosen at launch. |
| EVM fork | revm Cancun-equivalent (pinned in P2) | Never auto-follows upstream latest; fork bump = hard fork. |
| `EVM_NATIVE_SCALE` | `10^10` | sompi (8 dec) → wei (18 dec). Withdrawals must be exact multiples. |
| `EVM_GENESIS_STATE_ROOT` | zero (P1 placeholder) | P2 pins `keccak256(rlp(()))` empty-trie root. |
| Header preimage suffix (v2+ only) | `evm_state_root(32) ‖ evm_transactions_root(32) ‖ evm_receipts_root(32) ‖ evm_commitment_root(64)` | Appended after `pruning_point`. Frozen byte order. |
| EVM commitment domain | `b"MISAKA_EVM_HEADER"` | keyed BLAKE2b-512 over `EvmExecutionHeader`. |
| Subnetwork ids | `0x20` deposit, `0x21` withdraw-claim (reserved), `0x22` admin (reserved) | `subnets.rs`. |
| DB store prefixes | `201`–`210` | `database/registry.rs` (`EvmHeader`…`EvmBlockHashMap`). |
| Withdraw precompile | `0x…F002` (`MISAKA_WITHDRAW`) | `evm/mod.rs`. |
| Activation | `Params::evm_activation_daa_score` | `u64::MAX` = inert (all nets in P1). Target net = **testnet**; finite value set when the executor lands (P2+). |

**Circular-dependency rule (design §4.2):** the current L1 block hash and current EVM block hash are
**not** inputs to the EVM execution environment (the header hash already commits to the EVM result).
`blockhash`/`prevrandao` derive from `selected_parent` ancestry only.

---

## Version gating (the load-bearing correctness property)

The four EVM header fields are **always present** in the `Header` struct (defaulting to zero) but enter
the header-hash preimage **only when `header.version >= EVM_HEADER_VERSION`**
(`hashing::header::write_header_preimage`). Because genesis headers are v0 and live mined blocks are
v1 (both `< 2`), their preimage — and all three digests (legacy-32, identity-64, pre-PoW-64) — is
byte-identical to the pre-EVM protocol. `consensus-core::config::genesis::test_genesis_hashes` stays
green **with no constant changes**. (Mirrors the `merkle::*_pre_crescendo` version-gating precedent.)

On-disk: the consensus header is **bincode**-serialized (not borsh) via `database::access`. Adding
fields changes that layout, so `LATEST_DB_VERSION` is bumped `6 → 7` and old-shape DBs are rejected at
open time (clean resync, per [ADR-0001](0001-network-isolation.md)) rather than migrated.

---

## PQ-only reconciliation

revm pulls in secp256k1 (ecrecover + secp precompiles), which conflicts with the secp-free node
guarantee enforced by `scripts/pq-ci-guard.sh`. Resolution: the EVM **types** are always compiled and
are secp-free; the **executor** (revm) lands behind the `evm` cargo feature (default OFF). The default
`kaspad` build stays secp-free; an `--features evm` build opts into the EVM lane and secp. The EVM lane
is a separate signature domain from native UTXO ML-DSA-87 (design §1.2/§16). A PQ-EVM (no secp) is
explicitly out of scope.

---

## Implementation roadmap (design §17)

| Phase | Scope | Status |
|---|---|---|
| **P0** | Spec freeze (this ADR) | **Done** |
| **P1** | Consensus types: `EvmH256`/`EvmExecutionHeader`/`EvmExecutionPayload`; 4 header fields + version-gated preimage; block `evm_payload`; subnets; store prefixes; `evm_activation_daa_score`; body rule; `evm` feature declared | **Done** |
| P2 | revm executor behind `evm` (parent root → state/receipts roots); Ethereum state-transition + ACVP differential tests; pin fork + `EVM_GENESIS_STATE_ROOT` | Pending |
| P3 | EVM stores (201–210), multi-root state backend, canonical heads (no-replay on virtual change), pruning/GC | Pending |
| P4 | Deposit (subnet 0x20) extraction from acceptance data; withdraw precompile; UTXO-diff materialization; combined supply-invariant tests | Pending |
| P5 | EVM txpool, template builder (EVM roots + withdrawals in utxo_commitment), EIP-1559 basefee | Pending |
| P6 | `eth_*` JSON-RPC, logs, subscriptions, `safe`/`finalized` tags; wire EVM data through gRPC/p2p/RPC | Pending |
| P7 | Security/audit: DoS, state bloat, supply, reorg, RPC consistency | Pending |

### P1 surface (implemented)
- `crypto/hashes`: `EvmH256` (32-byte Ethereum H256, mirrors `Hash`).
- `consensus-core`: `constants::EVM_HEADER_VERSION`; `evm` module (`EvmExecutionPayload`,
  `EvmExecutionHeader`, `EvmAddress`, `EvmBloom`, frozen constants); 4 `Header` fields +
  `with_evm_commitments`; version-gated `write_header_preimage`; `Block`/`MutableBlock.evm_payload` +
  `with_evm_payload`; subnet ids + `is_evm_overlay`; `Params::evm_activation_daa_score` +
  `is_evm_active`; `RuleError::NonEmptyEvmPayloadBeforeActivation`.
- `consensus`: body-isolation rule `check_evm_payload` (pre-EVM header ⇒ empty payload);
  `LATEST_DB_VERSION 6→7`.
- `database`: store prefixes 201–210.
- Cargo `evm` feature declared (empty) on `kaspad`/`kaspa-consensus`/`kaspa-consensus-core`.

**Deferred to P2 (intentional):** non-zero EVM data is **not** carried over gRPC/p2p/RPC in P1 — the
convert layer round-trips the zero/empty P1 values via defaults; the wire extension lands with the
executor that first produces non-zero values, so wire + execution are tested together.

---

## Consequences
- A new hard fork; existing nets' genesis hashes are provably unchanged (version gate).
- Live non-EVM nets resync once (DB version bump).
- The node binary remains secp-free by default; EVM is an opt-in (`--features evm`) build.
- EVM TPS is bounded by the selected-parent lane, not DAG width.
