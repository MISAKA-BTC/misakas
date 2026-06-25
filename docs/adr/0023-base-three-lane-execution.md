# ADR-0023: MISAKA Base + Three Execution Lanes (PQ-EVM / ETH-compat / Proof-verified Parallel EVM)

## Status
**Proposed — design freeze, 2026-06-25. Nothing is implemented.** This ADR is a forward-looking
execution architecture for MISAKA L1. **Source design:**
[`docs/misaka-base-3lane-execution-design-v0.1.md`](../misaka-base-3lane-execution-design-v0.1.md)
(v0.1) — this ADR is the **code-grounded freeze** of that design against the current kaspa-pq tree;
every "§N" reference below points to a section of that document. It **does not supersede
[ADR-0020](0020-selected-parent-evm-lane.md) by replacement** — it **generalizes** it: the
selected-parent EVM lane that is live on testnet today becomes **Lane 2** of a four-layer model, and
the single-lane direction of ADR-0020 is the special case where only Lane 2 is active. No multi-lane
consensus rule may activate on any network until the hard precondition below is satisfied.

The **one** ADR-0020 property this ADR deliberately changes is its opt-in `--features evm` /
secp-free-default posture (ADR-0020 lines 31–32, 110–111, 152): making Lane 2 a mandatory core lane is
a **scoped supersession** of that single stance. Everything else in ADR-0020 carries forward verbatim
(see "Relationship to existing ADRs").

> **Hard precondition (§0 / §2.2 / Phase 0 below — non-negotiable gate).** The current EVM lane still
> seeds and persists a **full per-block EVM state snapshot clone** (prefix `206`), recomputes the
> **entire** keccak-MPT state root every block, and has no incremental trie. The C-01 flat/incremental
> backend (`EvmFlatAccount`=234, `EvmBlockStateRoot`=232, `EvmLatestStatePtr`=231,
> `flat_or_reconstruct_parent_snapshot`) exists in the tree **but is shadow-only and OFF by default**
> (`--evm-shadow-state-backend`, default `false`); when on it only cross-checks against the still-
> authoritative `206` snapshot and HALTs on divergence — it never replaces it. **The Phase-0
> state-backend / I-O remediation MUST land and become authoritative before any second lane is
> activated.** Multiplying today's full-clone path across lanes would only replicate the I/O problem.

This ADR adds a versioned multi-lane header commitment, a lane registry, a PQ-native transaction type +
auth registry, and a Lane-3 proof verifier + DA layer → a **multi-fork, multi-year program**, version-
gated the same way ADR-0020 was so existing genesis hashes stay byte-for-byte unchanged until
activation. It interacts with the PQ-only invariant ([ADR-0019](0019-mldsa87-migration.md)): Lane 2
keeps classical ECC, so the binary is **no longer fully secp-free** — the PQ guarantee is scoped to
Base + Lane 1.

---

## Context

### The goal — three lanes, three reasons

MISAKA wants to grow along three axes that a single EVM lane cannot serve at once:

- **PQ security adoption** — a lane whose native authorization, deposits, withdrawals, and official
  bridge depend on **ML-DSA-87 only**, no classical ECC, for long-horizon assets and PQ dApps.
- **Ethereum compatibility / ecosystem adoption** — keep the current secp256k1 EVM, its state, its
  chain ID, and unmodified MetaMask/Foundry/Hardhat tooling working.
- **Performance / professional execution** — an SVM/Sui-inspired parallel EVM whose full execution is
  done only by high-performance executors/provers, while normal nodes verify a validity proof + DA.

The four-layer model:

```text
Base   PQ consensus / DAG ordering / native PQ-UTXO / settlement / DA commitments
  └─ Lane 1  Primary Security Lane — Solidity-compatible PQ-EVM, ML-DSA-87 native authorization
       ├─ Lane 2  Compatibility Lane — the current ETH-compatible EVM, continued state-and-history
       └─ Lane 3  Performance Lane — parallel ECC-EVM; executors/provers run it, nodes verify proof+DA
```

### Lane security labels — fixed, normative (§1.4 / §5.2)

The per-lane security strings shown to users and in tooling are **frozen** and MUST be used exactly:

| Lane | Canonical security labels |
|---|---|
| Lane 1 | `PQ-AUTHENTICATED` · `PQ-SETTLED` |
| Lane 2 | `CLASSICAL-ECC` · `ETH-COMPATIBLE` |
| Lane 3 | `CLASSICAL-ECC` · `PROOF-VERIFIED` · `HIGH-PERFORMANCE` |

**Prohibition (§1.4 / §5.2): Lane 1 MUST NOT be labeled, advertised, or marketed as
"Ethereum-compatible."** Only Lane 2 carries `ETH-COMPATIBLE`. Lane 1 is a PQ-EVM with ML-DSA-87 native
auth; calling it Ethereum-compatible would misrepresent its security model and its tx/auth envelope.

### The current single-lane reality, grounded in code

The tree today implements exactly **one** lane (ADR-0020 / design v0.4) — there is **no** multi-lane
infrastructure. A whole-tree search for `LaneDescriptor`, `execution_lanes`, `execution_payloads_root`,
`LaneRegistry`, `MultiLane` returns **zero** matches. The grounding facts, authoritative over the prose
in this ADR:

- **Header commitments (confirmed).** `Header` carries two `Hash64` fields, `evm_payload_hash` and
  `evm_commitment_root` (`consensus/core/src/header.rs:192,196`), version-gated into the header-hash
  preimage only at `version >= EVM_HEADER_VERSION` (= **2**, `constants.rs:16`). Domains
  `b"EvmPayload64"` / `b"EvmCommitment64"`, keyed BLAKE2b-512 (`evm/mod.rs:111,114`). This is the
  exact machinery Lane work generalizes (one minor doc-comment drift: a helper docstring still says the
  retired label `MISAKA_EVM_COMMITMENT_V2`, but the bytes passed are `b"EvmCommitment64"`).
- **Constants (confirmed).** `EVM_CHAIN_ID = 0x4D_53_4B` ("MSK"), `EVM_GAS_LIMIT = 30_000_000`
  (= `MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK`), `MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK = 128 KiB`
  (`evm/mod.rs:64,150,169,172`).
- **Store prefixes.** EVM-named prefixes span **201..=234, non-contiguous**: contiguous `201..=211,213`
  (with **`212` being a *non-EVM* ADR-0022 overlay store** wedged in), then RPC/archive/flat stores
  `217,218,219,220 (§12 forward diff),221 (§12 checkpoint),222 (content-addressed code),231,232,234`.
  `214–216,223–230,233` are gaps/reserved (`database/src/registry.rs:122-216`). Prefix `206` is a full
  `EvmStateSnapshot` via `DbEvmStateStore` (confirmed).
- **Payload/gas situation (confirmed):** 128 KiB inclusion cap, 30M gas/anchor — these are the numbers
  the aggregate cap (FD-AGG below) is built on.
- **F003 ML-DSA-87 verify precompile (confirmed, inert).** `0x…F003` exists
  (`kaspa-evm/src/mldsa_verify.rs`), but `evm_f003_mldsa_verify_activation_daa_score = u64::MAX` on
  **all four** networks (`params.rs:1022/1119/1185/1206`); below the fence the handler is not
  registered, so a call is byte-identical to calling an empty account.
- **F004 PQ auth registry — NOT IN CODE.** There is **no** F004 precompile and **no** on-chain PQ auth
  registry anywhere in the tree. `PqAccountRecord`/`PqAuthRegistry` are design-only proposals
  (§5.5 / §5.10 of this ADR). `database/src/registry.rs` is an unrelated RocksDB store-prefix registry.
- **State backend remediation status — see the precondition.** Authoritative path still full-clone
  (`processes/evm/mod.rs:1502,1532`; `snapshot.rs:112-174`;
  `virtual_processor/processor.rs:1321`); state root still a full recompute (`state.rs:1-6,25-63`);
  the flat/incremental backend is shadow-only and off by default. Pruning of EVM block rows is
  **partial** (per-block prune of `206/211/203/219` exists, history-mode gated; code `222` never
  per-block pruned), but the §7.3 "finalized-diff GC over an incremental trie" model is **not** the
  implemented model. State sync is **pruning-point-snapshot only** (one full object, root-verified),
  not the §12.2 streaming chunked `LaneSnapshotManifestV1`.

ADR-0020's existing properties carry forward and ground Lane 2: EVM parent = `selected_parent(B)`,
mergeset delayed acceptance (B's payload accepted by its selected child), revm pinned to
`SpecId::SHANGHAI`, EIP-2718 + EIP-1559 + secp256k1 recovery, deposit subnet `0x20` / withdraw-claim
subnet `0x21` (reserved/unused — actual EVM→UTXO is the F002 precompile side-effect), and the
circular-dependency rule (current L1/EVM block hash is **not** an EVM env input). All confirmed.

### Rejected alternatives (§0.2 — explicit non-goals)

The following designs were **deliberately rejected**; they are on record so the negative decisions are
visible:

- **Per-lane DAG linearization / per-lane fork-choice or sequencer order** — rejected. Only Base
  linearizes (see FD-BASE, I-01).
- **Multiple lanes mutating one shared state** — rejected. `State1 ≠ State2 ≠ State3`, fully isolated
  (FD-SHARED's namespace split, I-03).
- **Synchronous cross-lane `CALL`** — rejected. Cross-lane interaction is async outbox/inbox only
  (FD-XLANE, I-11/I-12).
- **Lane-3 committee-multisig-as-validity** (a root adopted on executor signatures alone) — rejected.
  Validity requires a proof verifiable by a normal node (FD-L3PROOF, acceptance condition 5, I-15).
- **Unconditional per-lane replication of 30M gas / 128 KiB** — rejected. Lanes share one aggregate
  budget (FD-AGG, I-09).
- **Extending today's per-block full EVM state-snapshot clone to 3 lanes** — rejected. Empty lanes are
  O(1); no full-state clone or full-root recompute (FD-SHARED, I-10, and the Phase-0 gate).

---

## Decision — Base + three execution lanes

Adopt the four-layer model. The five **binding acceptance conditions** (§0) gate the whole program; an
implementation that violates any of them MUST NOT activate:

1. **Only Base linearizes the DAG.** No lane MAY have its own fork-choice or sequencer order.
2. **Lane 1's "parent block" is an *execution anchor*, not state inheritance.** Base's consensus parent
   stays `selected_parent`; Lane 1 state MUST NOT back-reference Base consensus and create a cycle.
3. **Lane 1 and Lane 2 share one EVM engine, one state backend, one receipt/log schema, one block
   environment, one RPC implementation.** Differences are limited to auth profile, chain/lane domain,
   precompile policy, and bridge policy.
4. **The Lane 1 + Lane 2 *aggregate* resource ceiling MUST NOT greatly exceed today's single-EVM
   ceiling.** Two lanes MUST NOT mean 2× CPU/RAM/I/O/bandwidth.
5. **A Lane 3 state root MUST NOT be adopted on executor signatures alone.** A validity proof verifiable
   by a normal node (or an equivalent objective fraud-proof) **and** complete Data Availability are
   preconditions for any production asset on Lane 3.

### Role / authorization matrix

| Layer | Security / purpose | Auth | Normal full node | State namespace |
|---|---|---|---|---|
| Base | PQ consensus, ordering, settlement | ML-DSA-87 native | Required | UTXO · lane registry · escrow |
| Lane 1 | Long-horizon protection, PQ dApps/assets | ML-DSA-87 only | Execute (mandatory core) | New PQ-EVM state |
| Lane 2 | Existing Ethereum UX / dApp compat | secp256k1 (EIP-2718) | Execute (mandatory core) | Continued current EVM state |
| Lane 3 | High-TPS games / NFT / partitioned DeFi | Ed25519 (initial) | **Verify proof + DA only** | New parallel-EVM / object state |

`account identity = (lane_id, address_or_account_id)` always. The same 20-byte alias in two lanes is
two different accounts. `State1 ≠ State2 ≠ State3`; the only native-supply bridge is **Base escrow**.

---

## The only consensus changes

Analogous to ADR-0021's "one consensus change" framing — here the consensus surface is larger, so it is
enumerated explicitly. Everything **not** in this list is ordinary EVM contracts, node-local runtime,
RPC, or tooling.

| # | Consensus change | Reuses | Genuinely new |
|---|---|---|---|
| 1 | **Generalize the header** from per-lane fields to **two versioned Merkle roots** `execution_payloads_root` (input) and `execution_results_root` (result), `Hash64`, header v3+ | ADR-0020 `evm_payload_hash`/`evm_commitment_root` split + keyed-BLAKE2b-512 commitment machinery | The Merkle-over-lanes leaf set; v3 version gate; canonical lane-sorted encoding |
| 2 | **Lane registry** (`LaneDescriptor`: id/ruleset/status/kind/auth/chain_id/synchronous/mandatory/activation) in Base consensus state | — | Entirely new; IDs never reused, stopped lanes kept as tombstones |
| 3 | **Lane 1 PQ tx type** (`PqEvmTransactionV1`) + native ML-DSA-87 sender auth before EVM execution | ADR-0019 ML-DSA-87 verifier (`kaspa-txscript`), the same one F003 uses; shared revm engine post-auth (`NormalizedEvmTx`) | New envelope, signature digest domain `MISAKA/PQ-EVM/TX/V1`, native-auth pipeline |
| 4 | **PQ Auth Registry (F004)** — consensus-native account/key registry; F004 predeploy is a read/write *view*, not the root of trust | — | **Entirely new — NOT in the tree today.** Must be implemented |
| 5 | **F003 / F004 activation** (lift the `u64::MAX` fences) | F003 verifier already in tree (inert) | Activation coordination only |
| 6 | **Lane 3 proof verifier + DA** — validity-proof statement, `verifier_key_hash` versioning, Base-inline DA (Phase DA-1) then sampling/erasure (DA-2) | ADR-0020 commitment domains for input roots | New proof system, DA protocol (DA-2 = its own ADR + audit) |
| 7 | **Cross-lane outbox/inbox** exactly-once message consume with Base escrow as native-supply root-of-truth | ADR-0020 deposit/withdraw system-op pattern | New message domain `MISAKA_XLANE_MSG_V1`, inbox dedupe |

What is **reused**, not rebuilt: the revm execution engine + `SpecId::SHANGHAI` schedule, the ADR-0019
ML-DSA-87 verifier, the ADR-0020 keyed-BLAKE2b-512 header-commitment construction, and the existing
deposit/withdraw consensus side-effect machinery.

---

## Frozen design decisions

Numbers marked **(candidate)** are testnet-candidate and frozen only after the §7.9 / §20.8 / §21.2
acceptance-and-benchmark gate (see "Release gates" below). §20 is the T-01..T-66 test plan; the
"do-not-activate-if-benchmark-misses" rule itself lives in §7.9.

- **FD-BASE — Base is the only sequencer.** Base does PoW/GHOSTDAG/DAA/UTXO/PQ-signature/DNS-FSL
  finality and native settlement; lanes are subordinate. `AcceptedLaneTxs_i(B)` is extracted from the
  same `sorted_mergeset(B)` for every lane; B's own payload is accepted by its selected child
  (off-by-one unified across lanes).
- **FD-ANCHOR — Lane 1 = Primary Execution Parent, clock/anchor only.** Lane 1 gives Lane 2/3 their
  execution epoch, timestamp, message epoch, and ruleset anchor. Lane 2/3 `primary_execution_parent` =
  the Lane-1 execution hash at the *same* anchor (`LaneExecutionHash = keyed_blake2b_512(key =
  "MISAKA_LANE_EXECUTION_V1", borsh(header))`). Lane 2/3 do **not** inherit Lane 1 account/storage
  state. Cross-lane input is delayed at least one anchor — no cycle.
- **FD-SHARED — Lane 1 & Lane 2 share one executor, state backend, and RPC.** Shared: revm version /
  SpecId / opcode table, block env, gas schedule, receipt/log generation, state DB interface, code
  cache, tracing, RPC read/sim engine, pruning/snapshot/state-sync, metrics. Per-lane: raw-tx
  decoder/authenticator, chain/lane id, precompile profile, account registry, fee-market state,
  bridge/security policy, mutable-state namespace. One process, one DB, one RPC stack; DB key =
  `(store_prefix, lane_id, key)`.
- **FD-CODE — content-addressed code dedup.** `shared_code_store[keccak(code)] = bytecode`;
  `lane_state[(lane_id, address)] = metadata + code_hash`. The same bytecode is never double-stored
  across Lane 1/2 (target: ≤5% code-storage duplication).
- **FD-AGG — aggregate resource cap; a second lane is NOT 2× VPS cost.** Lane 1 + Lane 2 share one
  budget, not two copies of today's. **(candidate)** total payload 128 KiB / anchor, total accepted gas
  30,000,000 / anchor; per-lane guaranteed floors 48 KiB & 12M gas each, shared borrowable 32 KiB &
  6M gas. Unused quota is borrowable in-anchor; floors hold under mutual congestion. Acceptance target
  (§7.9 / §21.2): core-node p95 CPU & peak RAM ≤ **1.35×** the current single-lane impl; empty second
  lane ≤ **3%** overhead. **If the §7.9 benchmark misses, do not activate.**
- **FD-BUDGET — compute-unit accounting, ML-DSA verify is not free.** `CoreBudget = {evm_gas,
  auth_compute_units, payload_bytes, state_read_units, state_write_units, receipt_log_bytes}`.
  ML-DSA-87 verify consumes calibrated `PQ_AUTH_COMPUTE_UNITS` from the shared core budget — enforced
  by consensus counters, not just documented. **(candidate)** Lane 1 caps: ≤16 native ML-DSA verifies /
  core block, ≤80 KiB native-auth bytes / core block, ≤32 calls / PQ tx, ≤8 F003 verifies / tx, ≤32
  F003 verifies / core block. ML-DSA-87 sizes are fixed by ADR-0019: **public key 2592 B, signature
  4627 B** (confirmed in `crypto/txscript/src/lib.rs:65,70`; F003 input length `1+2592+64+4627 = 7284`).
- **FD-L3PAR — Lane 3 is strict-parallel only.** No `LEGACY_SERIAL` class at mainnet launch. Initial
  scheme **Ed25519** (32 B key / 64 B sig, batch pre-verify); `signature_scheme` field reserved.
  An **enforced access manifest** (not EIP-2930, which permits out-of-list access) is mandatory:
  undeclared access → deterministic *failed receipt*, never a Base-block invalidation. Implicit access
  (sender nonce/balance, fee payer, value recipient, called-code account, CREATE/CREATE2 dest,
  precompiles) is rule-classified.
- **FD-L3OBJ — object model.** `Owned` (owner-signed, exact version, parallel across distinct objects),
  `Shared` (Base-canonical-ordered, writes serialized), `Immutable` (read-only, unbounded parallel).
  Deterministic conflict scheduler: thread count / wave order are local optimizations, but final
  effects MUST equal canonical semantics. Per-shared-object congestion budget + surcharge so a hot
  object does not raise the whole-lane base fee.
- **FD-L3PROOF — no committee-signature-as-validity.** A Lane 3 state root advances only on a valid
  proof bound to a canonical Base input range (`proof_system_id`/`circuit_version`/`verifier_key_hash`
  versioned; verifier upgrade = Base fork or explicit governance). Pairing-only proofs ⇒ Lane 3 is
  classical-security ⇒ Base native-asset escrow is capped; a long-term PQ settlement claim requires a
  hash-based or dual-proof migration. Delayed commitment: `B(n)` fixes the input root, `B(n+k)` verifies
  the proof. Cross-lane exit allowed only from `verified + Base finalized`.
- **FD-XLANE — async outbox/inbox, exactly-once.** No synchronous cross-lane `CALL`. Source commits an
  effect to its outbox root; Base confirms source-root status; destination consumes once at a later
  anchor (`message_id = H64("MISAKA_XLANE_MSG_V1" ‖ msg)`, inbox dedupes).
- **FD-SUPPLY — Base escrow is the native-supply root-of-truth.** Base UTXO → lane = escrow-lock then
  credit-after-verified-claim; lane → Base UTXO = source burn/debit then verified/finalized outbox then
  Base synthetic-UTXO materialization. No coin is represented in two lanes at once. Lane 1 → Lane 2/3
  is a **security downgrade** (ML-DSA-87 → secp256k1 / Ed25519) and wallets MUST display it; no
  auto-routing may downgrade security.
- **FD-HDR — header extension is two roots forever.** Per-lane header fields are never added; the header
  grows by a constant regardless of lane count (FD #1 above). Active lanes always emit a canonical
  (possibly empty) leaf; leaf omission MUST NOT change meaning.
- **FD-LABEL — lane security labels are frozen and Lane 1 is never "Ethereum-compatible."** Use the
  §1.4 canonical triple verbatim (Lane 1 = `PQ-AUTHENTICATED`/`PQ-SETTLED`; Lane 2 =
  `CLASSICAL-ECC`/`ETH-COMPATIBLE`; Lane 3 = `CLASSICAL-ECC`/`PROOF-VERIFIED`/`HIGH-PERFORMANCE`). Per
  §5.2, Lane 1 MUST NOT be advertised or labeled as Ethereum-compatible.

---

## Invariants (§16, I-01..I-24 — all carried)

```text
I-01 Only Base decides canonical DAG order.
I-02 Lane 2/3 bind to the Lane-1 execution hash at the same anchor.
I-03 Lane 1/2/3 mutable state is fully isolated.
I-04 The same raw tx cannot replay across lane / network / ruleset.
I-05 Lane 1 native sender auth accepts only an active PQ scheme.
I-06 Lane 1 official deposit destinations are limited to registered PQ identities.
I-07 Lane 1 official withdraw accepts only a PQ UTXO destination.
I-08 Lane 1/2 share an executor core but auth/precompile policy is explicitly separated.
I-09 The Lane 1+2 aggregate resource cap is consensus-enforced.
I-10 An empty lane is O(1) — no full-state clone or full-root recompute.
I-11 A cross-lane message is consumed only from a source verified/finalized root.
I-12 Cross-lane messages are exactly-once.
I-13 Native supply satisfies conservation across Base escrow + all lane balances.
I-14 Lane 3 executors/provers hold no ordering power.
I-15 No Lane 3 state advance without a valid proof.
I-16 No withdraw to Base/Lane 1/2 from Lane 3 pending / executed-unverified state.
I-17 A Lane 3 proof binds to a canonical Base input range.
I-18 A Lane 3 batch without DA never becomes verified.
I-19 A normal validator can verify Base safety without holding Lane 3 full state.
I-20 A Lane 3 halt does not halt Base / Lane 1 / Lane 2.
I-21 Lane 1 → Lane 2/3 moves are shown as a security downgrade in the wallet.
I-22 No per-lane header fields — extend by versioned roots.
I-23 Ruleset / crypto / proof-verifier upgrades go through versioned activation.
I-24 State snapshot import is size-bounded, streaming, and root-verified.
```

---

## Relationship to existing ADRs

- **[ADR-0020](0020-selected-parent-evm-lane.md) — generalized, with one scoped supersession.** The
  selected-parent EVM lane *becomes Lane 2*: same `selected_parent` parent rule, mergeset delayed
  acceptance, revm Shanghai, secp256k1, deposit/withdraw, chain ID `0x4D534B`, header version-gating.
  ADR-0020's single `evm_commitment_root` / `evm_payload_hash` split is the precedent the two versioned
  Merkle roots generalize. ADR-0020's own residuals (standalone EVM-row GC/pruning, full-snapshot state
  backend) are exactly the Phase-0 gate of this ADR. **The one ADR-0020 property NOT carried forward:**
  its opt-in `--features evm` / **secp-free-default** posture (ADR-0020 lines 31–32, 110–113, 152).
  Making Lane 2 a mandatory core lane means a node on an active network runs secp256k1 by default; that
  specific stance is **deliberately superseded** here. It is a scoped change to one property, not a
  reversal of ADR-0020 as a whole.
- **[ADR-0019](0019-mldsa87-migration.md) — PQ verifier reuse.** Lane 1 native auth and F003 both reuse
  the ADR-0019 ML-DSA-87 verifier and its frozen sizes (pk 2592 B, sig 4627 B). Lane 1 keeps the PQ-only
  guarantee in its own security domain; Lane 2 deliberately re-admits classical ECC.
- **[ADR-0010](0010-validator-node-architecture.md) — single-binary node-role policy preserved.** One
  `kaspad` binary, opt-in subsystems: `--core-lanes=1,2` (default on active net), `--lane3-proof-verifier=on`
  (default on), and optional `--enable-lane3-executor` / `--enable-lane3-prover` / `--enable-lane3-rpc`.
  A validator is still a full node + enabled subsystem; Lane 3 heavy deps (GPU/prover) live behind an
  optional cargo feature or sidecar, never in the normal binary. A normal validator does **not** run
  Lane 3 full execution/state/RPC but **must** run the Lane 3 proof verifier to admit Lane 3 roots.
- **[ADR-0021](0021-fact-settlement-layer.md) / [ADR-0022](0022-fsl-economic-design.md) — F003 contention,
  open decision.** ADR-0021 frames `0xF003` as "the only consensus change" for FSL; PREA design v1.1 §9
  versions the *same* `0xF003` ABI into version `0x01` (FSL generic Hash64 verify) and `0x02` (PREA
  key-bound root). This ADR adds a **third** consumer: Lane 1 native PQ auth (and the F004 registry).
  **Open decision:** `0xF003` is now shared by FSL, PREA, and Lane-1 native auth; the versioned-ABI and
  activation ownership of F003/F004 MUST be coordinated (no single design may claim sole ownership). See
  O-DEC below. (Note: two distinct files share the number 0022 on this branch —
  `0022-fsl-economic-design.md` and `0022-pruned-ibd-evm-overlay-snapshot.md`; the latter introduced the
  non-EVM prefix `212`.)

---

## Node sizing is a capacity target, not a consensus condition (§21.1)

**Normative principle (§21.1).** Node CPU and RAM sizing are **capacity-planning targets, not protocol
or consensus-membership conditions.** The protocol judges only objective facts: a valid proof delivered
**in deadline**, Data Availability, and objective equivocation. It does **not** judge a node by its
hardware. A slow or under-provisioned node may fall behind, but being under a sizing target is never by
itself a consensus violation, a disqualification, or a membership condition. The capacity targets below
exist so operators can provision adequately and so the FD-AGG/§7.9 benchmark has a reference, **not** to
gate consensus on hardware.

Capacity-planning reference (illustrative target table, §21.1):

| Role | Lanes run | Target reference |
|---|---|---|
| Core full node / validator | Base + Lane 1 + Lane 2 + Lane 3 proof verifier | 2-lane VPS class (the FD-AGG ≤1.35× / ≤3% target) |
| Lane 3 executor / prover | + Lane 3 full execution / proving | High-performance / GPU class, behind feature or sidecar |
| Light / read client | proof + header verification only | Minimal |

---

## Release gates (§21.2 — P0..P3, distinct from the phase rollout)

Separately from the Phase-0..7 rollout below, a **mainnet release-gate ladder** (§21.2) governs what is
allowed to go to production. The phases are *what activates when*; the P-gates are *what evidence is
required before each tier of asset risk is permitted*.

| Gate | Meaning | Required before |
|---|---|---|
| **P0 — multi-lane pre** | Phase-0 state-backend remediation authoritative; §7.9 / §20.8 benchmark passes the FD-AGG ≤1.35× / ≤3% targets; mergeset prefilter + RPC alloc caps + auth compute counters in place | Any second-lane consensus rule |
| **P1 — Lane 1+2** | Lane 2 re-tagged with full chain-id/state-root/history continuity; Lane 1 PQ-EVM + F003/F004 active; aggregate cap enforced; differential test vs old commitment clean | Real assets on Lane 1, default `eth_*` on Lane 2 |
| **P2 — Lane 3 testnet** | Lane 3 strict-parallel + access/object scheduler + proof verifier + DA-1, proven on testnet; **no canonical native asset**; shadow proof-compare clean | Lane 3 test tokens only |
| **P3 — Lane 3 production asset** | Audited proof + DA; native escrow cap + withdraw rate limit + emergency freeze; **≥2 independent executors/provers**; **≥90-day soak**; Base burden within target | Any production native asset on Lane 3 |

P-gates are evidence gates; the Phase table is the activation sequence. A phase MAY land in testnet
before its P-gate evidence exists, but the corresponding production risk tier MUST NOT be enabled until
its P-gate passes.

---

## Phased activation (§18 — Phase 0 is the gate)

| Phase | Scope | Gate / status |
|---|---|---|
| **0 — Prerequisite remediation** | Retire full-snapshot clone; **incremental state root**; pruning / state-sync; mergeset prefilter; RPC allocation caps; explicit auth resource counters | **Hard gate (= P0).** Flat backend exists but is shadow-only/off-by-default today; MUST become authoritative first. No lane work activates before this lands |
| **1 — Generalized lane types, Lane 2 only** | `execution_payloads_root` / `execution_results_root`; lane registry; current EVM run as **lane id 2 shadow**; old/new commitment differential test; Base header **v3** version gate. Lane 1/3 = canonical empty/inactive leaves | Mirrors the ADR-0020 shadow-then-activate pattern |
| **2 — Lane 2 migration** | Hard-fork anchor `H`: reclassify the current EVM to Lane 2 by **logical lane-id re-tag**, not a state copy. Chain ID / state root / history / index continuity; default `eth_*` endpoint unchanged; old binary explicitly stopped | Feeds **P1** |
| **3 — Lane 1 PQ-EVM activation** | New PQ genesis state; **F003/F004 activation**; `PqEvmTransactionV1`; strict registered deposit; PQ withdraw; batch tx; wallet/SDK. Enable the Lane 1+2 aggregate cap | F004 must be implemented (not in tree); completes **P1** |
| **4 — Lane 3 shadow mode** | Input ordering; Ed25519 auth; access/object scheduler; executor/prover network; **no canonical native asset**; normal node shadow-compares the proof verifier | Toward **P2** |
| **5 — Lane 3 test assets** | Proof-required verified root; Base-inline DA (DA-1); test token only; limited cross-lane messages | **P2** evidence |
| **6 — Lane 3 capped production** | Audited proof + DA; native escrow cap; withdraw rate limit; emergency freeze; ≥2 independent executors/provers; ≥90-day soak; Base burden within target | **P3** gate |
| **7 — Lane 3 DA scaling** | Erasure-coded DA; sampling/certificate (**own ADR + audit**); cap increases tied to measured propagation/availability | DA-2 = separate ADR |

---

## Open decisions (§22, O-01..O-15 — all carried; plus the F003 contention)

```text
O-01  Final chain-id values for Lane 1 / Lane 3.
O-02  The exact set of classical precompiles disabled in the Lane 1 profile.
O-03  PqEvmTransaction type byte + canonical encoding.
O-04  Atomic-batch MAX_CALLS and failure semantics.
O-05  Empirical calibration of PQ_AUTH_COMPUTE_UNITS.
O-06  Final Lane 1/2 guaranteed floors + shared-cap values.
O-07  When to add a Lane 3 scheme beyond Ed25519.
O-08  SDK generation of exact access manifests + simulation-retry rules.
O-09  How to map the Lane 3 object store onto EVM storage.
O-10  Proof system, verifier-key lifecycle, PQ migration of the proof.
O-11  Lane 3 DA-2 sampling / certificate scheme.
O-12  Lane 3 proof fee market.
O-13  Lane 3 native escrow cap + rate limit.
O-14  Whether the Base header v3 activation rides along with a re-genesis.
O-15  Physical migration vs logical alias of existing EVM history/index.
O-DEC F003/F004 versioned-ABI + activation ownership shared across FSL (ADR-0021),
      PREA (design v1.1 §9), and Lane-1 native auth — coordination required.
```

---

## Consequences

- **The binary is no longer fully secp-free.** Lane 2 is a mandatory core lane that keeps classical
  secp256k1; this **scoped-supersedes ADR-0020's secp-free-default / opt-in `--features evm` posture**
  (ADR-0020 lines 31–32, 110–113, 152) — `scripts/pq-ci-guard.sh`'s secp-free-default guarantee no
  longer holds for a node that runs the active network. The PQ guarantee is **scoped to Base + Lane 1**:
  native auth, official asset paths, deposits, and withdrawals are PQ; it does **not** mean every
  contract computation is NIST Category 5, and Lane 1 cannot stop a third-party dApp from implementing
  classical crypto in EVM bytecode. Per §1.4 / §5.2: Lane 2 must be labeled `CLASSICAL-ECC` /
  `ETH-COMPATIBLE`, **Lane 1 must never be advertised as Ethereum-compatible**, and Lane 1 → Lane 2 is a
  disclosed security downgrade.
- **Two-lane VPS-cost target is a release gate, not a hope.** The aggregate cap (FD-AGG) and the ≤1.35×
  / ≤3% acceptance targets are normative — if the §7.9 / §20.8 benchmark misses them, the second lane
  does not activate (= the P0 gate). This is only achievable after the Phase-0 state-backend
  remediation; activating multi-lane on top of today's full-clone path is explicitly forbidden. Note
  (§21.1): these are **capacity targets**, not consensus-membership conditions — the protocol still
  judges only in-deadline valid proof / DA / objective equivocation, never node hardware.
- **Lane 3 has real trust boundaries to surface.** Optimistic `executed` ≠ `verified`; proof-system /
  verifier-key compromise, DA withholding, executor/prover centralization, and shared-object hotspots
  are live risks. Correctness is enforced by proof, ordering by Base, availability by the DA protocol;
  wallets and docs MUST display the `executed`/`verified` gap and the trust class. No native asset
  crosses out of Lane 3 from unverified state.
- **This is a multi-year, multi-fork program, not a single fork.** Header v3 + registry + Lane-2 re-tag,
  Lane-1 PQ activation (incl. F004, which does not yet exist), Lane-3 proof + DA-1, then DA-2 are
  distinct coordinated activations, each version-gated so pre-activation genesis identity stays
  byte-for-byte unchanged, and each gated by its P0..P3 evidence tier. Nothing here is implemented; this
  ADR freezes the design and the gates.

---

## References
- **Source design:** [`docs/misaka-base-3lane-execution-design-v0.1.md`](../misaka-base-3lane-execution-design-v0.1.md)
  (v0.1, 2026-06-25). Every "§N" reference in this ADR (§0 acceptance conditions, §0.2 non-goals,
  §1.4 labels, §5.2 naming prohibition, §7.3/§7.9, §12.2, §16 invariants, §18 phases, §20 test plan /
  §20.8 benchmark matrix, §21.1 capacity / §21.2 release gates, §22 open decisions) is a section of
  that document; this ADR is its code-grounded freeze.
- EVM lane (generalized to Lane 2): [ADR-0020](0020-selected-parent-evm-lane.md),
  `docs/misaka-evm-design-v0.4.md`, `docs/misaka-evm-optimization-design-v0.1.md`.
- PQ scheme / sizes / verifier reuse: [ADR-0019](0019-mldsa87-migration.md),
  [ADR-0008](0008-hash64-consensus-identity.md), NIST FIPS 204 (ML-DSA).
- Node-role / single-binary policy: [ADR-0010](0010-validator-node-architecture.md).
- F003 contention: [ADR-0021](0021-fact-settlement-layer.md), [ADR-0022](0022-fsl-economic-design.md),
  `docs/misaka-prea-design-v1.1.md` (§9 versioned 0xF003 ABI).
- Grounding code: `consensus/core/src/{header.rs,constants.rs,evm/mod.rs,config/params.rs}`,
  `consensus/src/processes/evm/mod.rs`, `kaspa-evm/src/{executor,tx,snapshot,state,mldsa_verify}.rs`,
  `database/src/registry.rs`, `crypto/txscript/src/lib.rs`.
- Prior art (Lane 3): EIP-2718/2930/1559/7928; Solana transaction pipeline; Sui consensus + object
  model + local fee markets; Block-STM (arXiv:2203.06871).