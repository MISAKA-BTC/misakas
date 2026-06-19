# ADR-0020: Selected-Parent EVM Execution Lane on L1

## Status
Accepted & implemented through design **v0.4 (mergeset delayed acceptance)**; **ACTIVATED ON
TESTNET 2026-06-11**. All build phases are on `pr-19-s5f-…`: P0–P3 (types/executor/state), M10
(acceptance executor + hot path), P4 (UTXO↔EVM bridge: deposit-lock → claim → credit → F002
withdraw → synthetic UTXO), §15 (template + wire), §16 (EVM mempool + RPC + indexes), §14 (EVM-tx
P2P relay, protocol 100→101 back-compat), plus the N0 optimization pass (O1 sender cache, O2
parallel admission, O3 Evm reuse, O9 lane KPIs). Proven live first on a 3-node devnet-EVM mesh,
then on testnet: `TESTNET_PARAMS.evm_activation_daa_score = 0` (genesis-active) via a 3-host
re-genesis cutover — the testnet genesis hash is unchanged (`cf4c48fe…`), and post-activation
headers carry `EVM_HEADER_VERSION` (v2), so pre-EVM nodes and the old chain are version-isolated
in both directions. Devnet is likewise genesis-active; **mainnet/simnet stay inert (`u64::MAX`)**.

> **Design superseded by v0.4** — the unified design doc
> [`docs/misaka-evm-design-v0.4.md`](../misaka-evm-design-v0.4.md) replaces the v0.3 immediate-execution
> model with **mergeset delayed acceptance** (B's own payload is executed by its selected child), adds
> `evm_payload_hash` as a second header commitment, 5-class skip semantics, payload-miner fee routing,
> two-stage caps, and a non-decreasing timestamp clamp. The v0.4 migration deltas are listed in
> design §21; the sections below are the v0.2/v0.3 freeze, kept as the historical record of the
> consensus-safety analysis.

Source design: `MISAKA_Kaspa_L1_Selected_Parent_EVM_Design_v0.2_Audit_Revised.docx` +
`..._v0.3_DEX_Uniswap_Addendum.docx`. This ADR is the code-grounded freeze of that design against the
current kaspa-pq tree. The v0.2 audit elevated 6 consensus-safety rules and **collapsed the header to a
single `evm_commitment_root`** (the four-root v0.1 layout is gone — see "Version gating").

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
| EVM fork | revm `SpecId::SHANGHAI` (pinned P2) | London+ baseline runs Uniswap v2/v3 + current-solc contracts (design §19.2); Cancun/EIP-1153 (v4) is a deliberate later fork. Never auto-follows upstream; bump = hard fork. |
| `EVM_NATIVE_SCALE` | `10^10` | sompi (8 dec) → wei (18 dec). Withdrawals must be exact multiples. |
| `EVM_GENESIS_STATE_ROOT` | `keccak256(rlp(()))` empty-trie root (`56e81f17…b421`) | The P2 executor asserts an empty block reproduces it. |
| Header preimage suffix (v2+ only) | `evm_payload_hash(64)` then `evm_commitment_root(64)` | **TWO** keyed BLAKE2b-512 roots, appended in that order after `pruning_point` (design v0.4 §4.1/§4.3 — superseded the v0.2 single-root layout). Frozen byte order. |
| EVM commitment domains | `b"EvmPayload64"` · `b"EvmCommitment64"` | `EvmPayload64` keys the block's raw `EvmExecutionPayload` (→ `evm_payload_hash`); `EvmCommitment64` keys the body-side `EvmExecutionHeader` (state/tx/receipts/system-ops/withdrawals/deposit-claim roots, gas, basefee, logs bloom, evm_number, evm_timestamp_sec, burn accumulator → `evm_commitment_root`). The earlier `MISAKA_EVM_COMMITMENT_V2` domain is retired. |
| Subnetwork ids | `0x20` deposit, `0x21` withdraw-claim (reserved), `0x22` admin (reserved) | `subnets.rs`. |
| DB store prefixes | `201`–`210` | `database/registry.rs` (`EvmHeader`…`EvmBlockHashMap`). |
| Withdraw precompile | `0x…F002` (`MISAKA_WITHDRAW`) | `evm/mod.rs`. |
| Activation | `Params::evm_activation_daa_score` | **testnet = 0 (genesis-active since 2026-06-11)**, devnet = 0; mainnet/simnet = `u64::MAX` (inert). Activation gates header v2 (`EVM_HEADER_VERSION`) — see Status. |

**Circular-dependency rule (design §4.2):** the current L1 block hash and current EVM block hash are
**not** inputs to the EVM execution environment (the header hash already commits to the EVM result).
`blockhash`/`prevrandao` derive from `selected_parent` ancestry only.

---

## Version gating (the load-bearing correctness property)

The single `evm_commitment_root` field is **always present** in the `Header` struct (defaulting to
zero) but enters the header-hash preimage **only when `header.version >= EVM_HEADER_VERSION`**
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
| **P1** | Consensus types: `EvmH256`/`EvmExecutionHeader`/`EvmExecutionPayload` + deposit/withdraw ops; single `evm_commitment_root` + version-gated preimage; block `evm_payload`; subnets; store prefixes; `evm_activation_daa_score`; body rule; `evm` feature declared | **Done** |
| P2 | revm `SpecId::SHANGHAI` executor behind `evm` (parent state root → keccak state/tx/receipts roots); deterministic env (number/ts-clamp/prevrandao/EIP-1559 basefee); deposit-claim credit; F002 withdraw precompile; commitment matches `EvmExecutionHeader.commitment_root()`; differential tests | **Done** (live on testnet-10) |
| P3 | EVM stores (201–210), multi-root state backend, canonical heads (no-replay on virtual change), pruning/GC | **Done** (store-side; standalone GC/pruning of EVM rows still pending) |
| P4 | Deposit (subnet 0x20) extraction from acceptance data; withdraw precompile; UTXO-diff materialization; combined supply-invariant tests | **Done** — deposit-lock → `submitEvmDepositClaim` → refund + F002 withdraw all live (incl. the claim-retry + same-generation TOCTOU fix, commit `51ece4d`) |
| P5 | EVM txpool, template builder (EVM roots + withdrawals in utxo_commitment), EIP-1559 basefee | **Done** — own-payload EVM mempool + claim queue + template fold |
| P6 | `eth_*` JSON-RPC, logs, subscriptions, `safe`/`finalized` tags; wire EVM data through gRPC/p2p/RPC | **In progress** — feature-gated Ethereum JSON-RPC adapter (`rpc/eth`, `--features evm`): identity/chain/state/`eth_call`/`estimateGas` done; `eth_sendRawTransaction`/receipts/blocks + logs/subscriptions WIP |
| P7 | Security/audit: DoS, state bloat, supply, reorg, RPC consistency | **Done** — external EVM-bridge audit (F1–F6) remediated (commit `74d5442`); state-bloat GC remains the open item |

### P1 surface (implemented)
- `crypto/hashes`: `EvmH256` (32-byte Ethereum H256, mirrors `Hash`).
- `consensus-core`: `constants::EVM_HEADER_VERSION`; `evm` module (`EvmExecutionPayload`,
  `EvmExecutionHeader`, deposit/withdraw op types, `EvmAddress`, `EvmBloom`, frozen constants); single
  `Header::evm_commitment_root` + `with_evm_commitment`; version-gated `write_header_preimage`;
  `Block`/`MutableBlock.evm_payload` +
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
