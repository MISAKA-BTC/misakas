# MISAKA Strict PQ-Rooted EVM Deposit — Design v0.1 (audit QR-C01 remediation)

**Status:** Design. Multi-session build. **Consensus fork** (fenced, u64::MAX-inert).
**Addresses:** quantum-resistance audit QR-C01 (Critical) + QR-H01 (F003 activation gating).
**Builds on:** PREA design v1.1 (`docs/misaka-prea-design-v1.1.md` §8–§11/§24/§25), which
SPECIFIES these controls; this doc reconciles that spec with the current code (gap analysis)
and sequences the fenced consensus changes. The F003 ML-DSA precompile is already implemented
(inert).

---

## 1. Problem (audit QR-C01)

A native UTXO authorized by ML-DSA-87, when bridged UTXO→EVM, is credited to an **arbitrary
20-byte EVM address** and thereafter authorized by **secp256k1 ECDSA** — a quantum downgrade.
Confirmed in code:

- The deposit lock holds the 20-byte EVM destination as **plaintext, unvalidated**
  (`crypto/txscript/src/script_class.rs:295-326`, `evm_deposit_lock_script` / `EvmDepositLockFields`).
- Claim validation checks only amount / timeout / tip — **no check that the destination is a
  registered PQ account** (`consensus/src/processes/evm/mod.rs:90-116`, `validate_one_deposit_claim`).
- The executor credits the wei to **any** recovered address
  (`kaspa-evm/src/executor.rs:150-175`, `credit_balance(to_revm_address(claim.evm_address), …)`).
- An EVM tx's sender is recovered by **secp256k1 ECDSA** with no PQ-account gate
  (`kaspa-evm/src/tx.rs:398-433`, `decode_tx_to_env` → `recover_signer_cached`).

So depositing a PQ-protected UTXO to an EOA (or arbitrary contract) drops to classical
security. The PREA-specified remedies — **F004 registry, strict deposit, direct-ECDSA reject** —
are **entirely unimplemented** (zero grep hits for `F004` / `PqAuthRegistry` / `is_pq_account`).

## 2. The three controls (and F003 prerequisite)

| Control | What it enforces | Where it hooks | PREA |
|---|---|---|---|
| **(0) F003 activation** | ML-DSA root verify is usable on-chain (so PQ accounts can be *operated* at all) | already implemented, `mldsa_verify.rs`; fence `evm_f003_mldsa_verify_activation` = u64::MAX | §9 |
| **(1) F004 PQ-auth registry** | the on-chain data layer: `address → PqAccountRecord` + `is_pq_account(addr)` | new EVM-state predeploy at `0x…F004` | §10 |
| **(2) Strict deposit** | a deposit may land ONLY at an address registered as `PqSmartAccountV1` in F004 (at selected-parent) | new strict lock script class + claim validator with registry access | §11.2/§11.3 |
| **(3) Direct-ECDSA reject** | a registered PQ account can NEVER be authorized by a recovered secp256k1 tx (defense-in-depth) | executor per-tx, class-2 skip | §8.4/§24 |

(1) is the shared data layer; (2) stops new downgraded deposits; (3) stops operating a
registered PQ account classically. All three are **fenced** and inert until a coordinated
activation. F003 (0) is the prerequisite — without it PQ accounts cannot execute, so activation
of (1)–(3) is a single coordinated release gate (audit QR-H01).

## 3. F004 — PQ Auth Registry (control 1)

An EVM-state-resident **system predeploy** at `0x…F004` (parallel to F002/F003), holding one
record per registered account. Per PREA §10:

```
enum EvmAccountAuthType { LegacyEcdsa = 0, PqSmartAccountV1 = 1 }
struct PqAccountRecord {
    account:           EvmAddress,   // the smart-account (CREATE2) address
    account_version:   u32,
    factory:           EvmAddress,   // the deploying factory
    init_code_hash:    EvmH256,      // CREATE2 init-code hash (binds the address to code)
    vault_owner_payload64: [u8;64],  // ML-DSA-87 vault-owner key payload (BLAKE2b-512)
    operational_root_hash: EvmH256,  // current ML-DSA-87 operational root
    auth_type:         EvmAccountAuthType,
    // recovery / policy fields per PREA §10
}
```

- **Storage:** records live in F004's EVM storage (so they participate in `state_root`, reorg via
  the canonical-pointer switch — no bespoke revert, PREA §25.1). Keyed by account address.
- **Registration** is itself a transaction that calls F004 with `(factory, init_code_hash,
  vault_owner_payload64, operational_root, ML-DSA proof)`; F004 **verifies the ML-DSA signature +
  key-payload binding via F003** (PREA §8.3 step 6), and that `account == CREATE2(factory,
  init_code_hash)`. A registration is NOT claimable in the same block (§5).
- **Read surface:** `is_pq_account(state, addr) -> bool` and `record(state, addr) -> Option<Record>`
  — pure reads against the in-execution CacheDB / a selected-parent EVM-state view. Used by
  controls (2) and (3).
- **Implementation:** new `kaspa-evm/src/pq_auth_registry.rs` (call-frame interception like F002,
  registered in `precompiles.rs::register_all_misaka_precompiles` behind `f004_active`), plus the
  record codec in `consensus/core/src/evm/` (secp-free types). Genesis-neutral (no predeploy below
  the fence).

## 4. Strict deposit (control 2) — the architectural change

A new **strict lock script class** commits the target PQ account record; claim validation binds
the deposit to that account's registry record at selected-parent.

- **Lock (`crypto/txscript/src/script_class.rs`):** add `ScriptClass::EvmPqDepositLock` (distinct
  discriminant from the legacy `EvmDepositLock`) with `EvmPqDepositLockFields { pq_account: [u8;20],
  pq_account_record_hash: [u8;32], timeout_daa_score, claim_tip_sompi }`. The committed
  `record_hash` binds the deposit to a SPECIFIC account record (a rotation/change → stale hash →
  the deposit is unclaimable and refunds via the existing timeout path → no fund loss).
- **Claim (`consensus/src/processes/evm/mod.rs:90`):** strict-mode `validate_one_deposit_claim`
  additionally checks, against the **selected-parent EVM registry state**: (a) `pq_account` is
  registered as `PqSmartAccountV1`, and (b) `registry.record(pq_account).hash == lock.record_hash`.
- **The plumbing change (central):** the claim validator today receives **only** the UTXO view
  (`processor.rs:756-759`: `claim_view = selected_parent_utxo_view.compose(mergeset_diff)`). Strict
  validation needs a read-only **selected-parent EVM-state (registry) view** threaded in alongside
  the UTXO view. Two viable shapes:
  - **A — registry view at claim time (preferred):** materialize a lightweight read-only registry
    view from the selected-parent EVM state and pass it to `validate_evm_deposit_claims`. The check
    happens BEFORE the UTXO lock is consumed (so a non-PQ destination never spends the lock).
  - **B — commitment + executor confirm:** the lock commits `record_hash`; the claim validator does
    only the cheap structural check; the executor (which holds the CacheDB) re-confirms the record
    against F004 before `credit_balance`. **Rejected:** if the executor skips the credit but the
    consensus layer already consumed the UTXO lock, the lock is spent with no credit (supply/fund
    loss). So the registry check MUST gate UTXO consumption ⇒ shape A.
- **Legacy locks** remain valid below the fence and continue to refund/claim as today; above the
  fence, in PQ-only deployments, the wallet/RPC refuses to *generate* legacy locks (§8).

## 5. Direct-ECDSA reject (control 3)

In the executor's per-tx loop (`executor.rs` v1 `218-249` / v2 `398-448`), AFTER
`decode_tx_to_env` yields the recovered secp256k1 sender, query the in-execution CacheDB:
`if pq_reject_active && is_pq_account(state_db, txenv.caller) { skip class-2 }` (a deterministic
skip — no nonce bump, no gas, no state change — using the **existing** class-2 machinery at
`executor.rs:376-382`, NOT a block-invalid error). The CacheDB already holds the post-system-op
selected-parent EVM state, so the registry read is in-scope and consensus-deterministic. The
identical check must run in the **simulator** (`sim.rs`) so `eth_call`/`eth_estimateGas` agree
(c==v parity, PREA §24). A **mempool soft-reject** (`mining/src/evm_mempool.rs`) is advisory only;
consensus is the authority.

## 6. Consensus, fences, reorg

- **It is a fork** (alters deposit/claim/tx-validation rules) ⇒ three new u64::MAX-inert fences,
  threaded like the existing ones (`params.rs:384-391` → `EvmBlockInput` → executor compare
  `executor.rs:206-208`): `evm_f004_pq_auth_registry_activation_daa_score`,
  `evm_strict_pq_deposit_activation_daa_score`, `evm_pq_direct_ecdsa_rejection_activation_daa_score`.
  All four (incl. F003) activate together as one coordinated release gate (QR-H01). Default/below
  the fence: byte-identical to today (genesis unchanged; the §22-style inert proof applies).
- **Same-block registration (deterministic, PREA §11.3):** system ops apply BEFORE user txs
  (`executor.rs:150-175`), so a claim cannot reference a registration created in its own block;
  strict claims reference **selected_parent(B)** registry only ⇒ no circular dependency.
- **Reorg:** the F004 registry lives in EVM state (canonical-pointer-switch reorg, no DB revert);
  deposit effects live in the per-block UTXO diff (existing revert machinery). On a reorg the
  selected-parent EVM state (hence the registry view) changes, so the strict-claim and direct-ECDSA
  skip decisions **re-evaluate** — consensus-safe because they are deterministic functions of
  selected_parent(B) that every honest node computes identically. A strict claim that validates on
  one fork and reorgs out refunds via the lock timeout (the lock is unspent on the losing fork).
- **c==v:** registry reads must be identical in executor + simulator + every verifier; reuse the
  single `register_all_misaka_precompiles` entry point + one `is_pq_account` helper.

## 7. Slice plan (each fenced/inert; offline-verifiable where noted)

1. **S1 — F004 record types + codec** (consensus-core, secp-free): `EvmAccountAuthType`,
   `PqAccountRecord`, `record_hash`, borsh byte-stability guard. *Offline.*
2. **S2 — F004 registry storage + reads** (`kaspa-evm/src/pq_auth_registry.rs`): record read/write
   against CacheDB, `is_pq_account` / `record`. *Offline (synthetic CacheDB).*
3. **S3 — F004 handler + registration** (call-frame interception, F003-verified registration +
   CREATE2 binding), registered behind `f004_active`. Genesis-neutral. *Offline + evm tests.*
4. **S4 — `evm_f004_*` fence** wiring (params → EvmBlockInput → executor; 4 networks u64::MAX;
   inert proof: genesis unchanged, below-fence byte-identical). *Offline.*
5. **S5 — direct-ECDSA reject** (control 3): executor per-tx `is_pq_account(caller)` class-2 skip +
   sim parity + fence `evm_pq_direct_ecdsa_rejection_activation`. *Offline (executor tests:
   registered → skip; unregistered → execute; reorg re-eval).*
6. **S6 — strict deposit lock script class** (`EvmPqDepositLock` + fields + parse). *Offline (script
   vectors).*
7. **S7 — selected-parent registry view into claim validation** (the shape-A plumbing): thread a
   read-only registry view; strict `validate_one_deposit_claim` (registered + record_hash match);
   fence `evm_strict_pq_deposit_activation`. *Offline (claim validator tests) + consensus evm tests.*
8. **S8 — interim disclosure + PQ-only guard** (§8): RPC/UI/README mark EVM deposit as a non-PQ
   boundary below activation; PQ-only wallet mode refuses legacy-lock generation. *Offline.*
9. **S9 — adversarial review + activation params + testnet activation** (coordinated with F003):
   fuzz/invariant + the acceptance suite (§9), then a testnet-first finite DAA on all four fences.

## 8. Interim mitigation (before activation)

Per the audit: until the fences activate, **do not claim end-to-end PQ for EVM assets.** Mark the
EVM-deposit path as a non-PQ boundary in RPC capability metadata, wallet UI, and README; in a
PQ-only wallet profile, **refuse to generate a (legacy) deposit lock** (the wallet change is
independent of the consensus fork and can ship first). This closes the *representation* risk
immediately while the consensus controls are built and activated.

## 9. Acceptance criteria (from QR-C01 受入条件)

- An arbitrary-EOA deposit (legacy lock to a non-registered address) is **rejected/unclaimable**
  above the strict fence; only addresses registered `PqSmartAccountV1` in F004 at selected-parent
  receive a strict claim.
- A strict claim whose `record_hash` ≠ the registry record at selected-parent is **rejected**
  (and the lock refunds via timeout).
- A registration is **not** referenceable by a claim in the **same** block.
- A direct secp256k1 tx whose recovered sender is a registered PQ account is a **class-2 skip** on
  **every** path — consensus, mempool, simulator, IBD, and across reorgs.
- Below every fence: genesis hashes unchanged, committed bytes byte-identical (inert proof).
- Fork-boundary + reorg: a strict claim valid on one fork that reorgs out refunds; no double-credit;
  supply invariant (I7) preserved.

## 10. Relationship to F003, C-01, and the other audits

- **F003** (control 0) is implemented + inert; QR-C01's activation bundles F003+F004+strict+reject
  into one coordinated gate (QR-H01).
- **C-01** (state backend) is an orthogonal, consensus-NEUTRAL refactor (separate doc); the F004
  registry simply becomes more state the new backend stores — no interaction beyond that.
- This design does **not** make EVM *sessions* PQ (those remain secp256k1 — audit QR-M09, a
  separate disclosure/roadmap item); it makes the deposit destination and root authorization PQ,
  which is what QR-C01 requires.

## 11. Open questions

1. **Registry-view plumbing (S7).** Confirm the cleanest way to expose a read-only selected-parent
   EVM registry view to the consensus claim validator (a dedicated `EvmRegistryView` derived from
   the selected-parent EVM state snapshot vs a `ComposedView`). Must avoid pulling revm into the
   secp-free consensus path — likely a small borsh record read, not a full CacheDB.
2. **Record-hash domain.** Freeze the `record_hash` preimage (which fields, ordering, domain
   separation) so a lock binds to exactly the intended record version; add a byte-stability guard.
3. **Activation governance.** F003+F004+strict+reject as one gate vs staged — recommend one gate
   (a half-activated state is a downgrade trap). Testnet-first with the full acceptance suite.
4. **Legacy-lock sunset.** Whether legacy `EvmDepositLock` remains permitted (to non-registered
   EOAs) above the fence for non-PQ use, or is rejected in PQ-only deployments only (wallet-level
   vs consensus-level) — recommend wallet-level refusal + consensus-level strict-only for the
   PQ-rooted path, keeping legacy for explicitly-classical use.
