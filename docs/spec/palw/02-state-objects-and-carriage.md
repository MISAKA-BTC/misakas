# PALW spec — 02. State, objects and carriage

> **Normative.** This chapter states the rules as they are on each network today. Reasoning:
> [design/palw/state-and-carriage.md](../../design/palw/state-and-carriage.md). The code is the
> truth, and disagreements are listed in [divergences.md](divergences.md).

**Applies to:** mainnet (not active: PALW disabled) · testnet-12 (from genesis)
**Reconciled with code at:** `55a7be02f` (2026-09-27)
**Principles served:** P3 and P4 (one state, one ordering, nothing that depends on representation).

This chapter defines the data every other chapter refers to: the PALW state (the "fold"), the objects
that change it and the transactions that carry them, the two validation layers, the order in which a
block applies what it merges, and what the header commits to.

## 2.1 The PALW state

- **PALW-ST-2 (mode).** A network MUST be in exactly one PALW mode (`PalwConsensusMode`): `Disabled`,
  `LegacyTn11` (the retired algo-4 lane) or `ConsensusV2(params)`. `ConsensusV2` carries the whole
  ruleset or none of it. There are no partial PALW fences outside the bundle.
- **PALW-ST-3 (the fold).** The PALW state (`PalwStateBookV2`) MUST be a pure function of the selected
  chain and its mergesets. It changes only in `apply_block` / `apply_block_with_work`, one block at a
  time, and every write is a delta entry that a reorg reverts. It holds bonds, classes, claims and
  panels, locks and liability records, vesting rows, court and DA sessions, market rows, and the
  pending-payout queue.
- **PALW-ST-4 (candidate-scoped reads).** Every consensus read about a candidate block — the weight,
  admission, the panel draw, the anchor facts — MUST be taken on that candidate chain's own state at
  the stated point (its parent's state, or the pre-object base of 08 PALW-VF-8). A read MUST NOT use
  the node's current sink state. Equal DAGs MUST give equal weights.
- **PALW-ST-1 (schema v22).** This lineage writes `PALW_STATE_V2_VERSION` 22, and the version is hashed
  first into every state root. v22 appends:
  - to the claim: `job_identity` and `claim.rcore`;
  - to the liability record: `job_identity`, `free_prompt`, `trace_root`, `segment_count`,
    `licence_door`, `basis_k`, `g_res_sompi` and `escrowed_reward`;
  - to the lock: `attested` and `segments`;
  - to the consumed offence: `collected` and `claim_id`;
  - new rooted maps: `vesting`, `reporter_rewards`, `reward_pending`, `reporter_commitments`,
    `da_sessions`, `da_claims` and `withholding_strikes`, plus the counters;
  - offence kinds 3–6, contradictions 9–13, the receipt verdict `Sampled`, and the void reasons
    `UnavailableQuorum` and `NotReplayBacked`;
  - object tags 53–56;
  - delta entries from 66 onward.

  Everything is appended. No enum variant is inserted mid-enum, because Borsh discriminants are chain
  bytes. Fold *behaviour* is fence-gated, but record *encodings* change on every network running
  this binary.

**Sources:** ADR-0042 D1, D2, D5; ADR-0152 §6. **Code:** `core/palw_mode_v2.rs` (`PalwConsensusMode`),
`core/palw_state_v2.rs` (`PalwStateBookV2::apply_block`, `PalwStateDeltaV2`), `cons/processes/palw_state_v2_sync.rs`,
`cons/processes/palw_state_walk.rs`.

## 2.2 Objects and their carriage

- **PALW-ST-5 (objects).** Every state change other than the block's own attempt and the deadline
  sweeps MUST be a `PalwConsensusObjectV2`, Borsh-encoded, with tags assigned in declaration order and
  only ever appended. The tags group as follows:
  - bonds: 0–2 (`BondRegistered`, `BondCapabilityDeclared`, `BondRetireRequested`);
  - classes and certification: 3, 4, 13, 14, 48, 58;
  - panel and licence: 5, 6, 11, 47, 49, 50, 52, 56;
  - courts: 7–10, 19–23, 38, 42, 43, 57;
  - the free prompt: 12;
  - DA: 17, 18, 44, 45, 55;
  - objects and artifacts: 15, 16;
  - the market: 24–37;
  - shards: 39–41;
  - offences: 46, 51;
  - reporter: 53, 54.
- **PALW-ST-6 (timeouts are never carried).** Timeout edges — bind and receipt timeouts, the end of the
  challenge window, retirement completion — MUST be derived from the deadline index and the block's
  DAA inside the transition. An object claiming that a timeout happened is not a valid object.
- **PALW-ST-7 (carriage).** PALW objects ride transactions in the PALW subnetwork band:
  - lifecycle objects (binding, licensing, court moves, DA, offences, market, registry and the rest
    of `PalwConsensusObjectV2`) ride `SUBNETWORK_ID_PALW_LIFECYCLE` (`0x4b`);
  - free-prompt commitments ride `SUBNETWORK_ID_PALW_FP_COMMITMENT` (`0x4a`);
  - the earlier bands `0x40`–`0x49` are the V1/Stage-1 carriage.

  Each band has its own codec and acceptance rules, and no band's payload may be validated by
  another's validator.
- **PALW-ST-8 (a bond rides its collateral).** A `BondRegistered` MUST be bound to its carrier
  (`palw_bond_registration_binds_its_carrier_v2`). The bond's outpoint is an output of the carrying
  transaction, holding at least the declared collateral, and paying to the P2PKH the registration
  names as its payee. The registration is verified under the operator key it carries, and the
  operator id is derived from that key (`palw_operator_id_v2`).
- **PALW-ST-9 (panels are derived).** A panel is derived by the draw (08) and bound by `PanelBound`.
  Receipts and verdicts are carried with their evidence. A receipt is evidence, not a vote.
- **PALW-ST-10 (mass).** Every carrier MUST fit the standard mass cap (480,000). Payloads too large
  for one carrier are split by their own rule, for example a seed paid in several transactions
  (15 §15.2).

**Sources:** ADR-0046 D1–D5 (with its design band `0x50`–`0x55` replaced in code by
`SUBNETWORK_ID_PALW_LIFECYCLE`), ADR-0029 (superseded shape). **Code:** `consensus/core/src/subnets.rs`,
`core/palw_lifecycle_objects_v2.rs`, `core/palw_fp_objects_v3.rs`, `core/palw_carriage.rs`,
`core/palw_state_v2.rs` (`PalwConsensusObjectV2`).

## 2.3 Two validation layers

- **PALW-ST-11.** Stateless shape — decoding, version, sizes, and signatures checkable without state —
  MUST be checked at transaction isolation validation. A malformed carrier makes its block invalid.
- **PALW-ST-12.** Stateful admission — does the bond exist, is the anchor mature, does the quorum hold
  at this chain point — MUST be checked at acceptance against the candidate chain's state. A carrier
  that fails it is **skipped**: it is not accepted, and its block stays valid. Two honest carriers can
  race, and a block builder must not be invalidated by mempool timing it cannot see.
- **PALW-ST-13.** The object list a chain block hands to the transition MUST be exactly the
  accepted-carrier list. Only the header-carried attempt can still fail the transition, and its
  failure is the block's own.

**Sources:** ADR-0046 D2. **Code:** `cons/pipeline/virtual_processor/processor.rs` (the acceptance
walk), `tx_validation_in_isolation`.

## 2.4 Acceptance order

- **PALW-ST-14.** Objects MUST take effect in acceptance order: the selected chain in order, and each
  block's mergeset in GHOSTDAG order. Within a block the fold runs these steps:
  1. drain the payout queue;
  2. the EVM and execution-lane boundaries;
  3. deadline and session sweeps (step 2);
  4. retarget and budgets;
  5. objects in acceptance order (step 3), then carrier refunds and the EVM market;
  6. vesting maturity (step 3d);
  7. activation and budgets;
  8. own and merged work (step 4);
  9. the anchor-block voids (step 4c).
- **PALW-ST-15 (merged work counts).** Claims carried by merged, non-chain blues MUST enter the state.
- **PALW-ST-16.** From DAA 750, a merging block MUST apply a tied round lane parents first
  (`palw_lane_accept_parents_first`, 12 §12.3).

**Sources:** ADR-0046 D5, ADR-0058, ADR-0152 V-4 (the fold order), ADR-0154.

## 2.5 What the header commits

- **PALW-ST-17.** A V2 header MUST commit the PALW state root. The root's hash ordering MUST have no
  challenge↔commitment cycle, and the None-root rule applies. *Sources:* ADR-0043 §1–§4, its
  version-17 amendment (ADR-0078). *Code:* `core/palw_block_commitment.rs`, `core/palw_state_chunk_map.rs`.
- **PALW-ST-18.** Pruning MUST keep an authenticated commitment to the PALW state, so a pruned node can
  verify the state it resumes from. *Sources:* ADR-0042 D5, ADR-0041 (13 §13.2).

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | from genesis. Parents-first round-lane acceptance from DAA 750 |
