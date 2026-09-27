# PALW spec — 02. State, objects and carriage

> **Skeleton (Phase 1, 2026-09-27).** [00-index.md](00-index.md) gives the conventions.

**Purpose.** This chapter defines the data every other chapter refers to. It covers the PALW state
(the "fold"), the object kinds and the transactions that carry them, the two validation layers, the
order in which a block applies what it merges, and what the header commits to. Chapters 03–15 say
what each object *means*. This chapter says how objects travel and when they take effect.

**Principles served:** P3, P4 (one state, one ordering, no dependence on representation).

## 2.1 The PALW state

- [ ] Define the state (`PalwStateBookV2`) as a pure function of the selected chain and its mergesets:
  bonds, classes, claims, locks, vesting rows, market rows and pending payouts. Name the only entry
  points: `apply_block` and `apply_block_with_work`. *Sources:* 0042 D2/D5, 0046. *Code:*
  `core/palw_state_v2.rs` (`apply_block`, `apply_block_with_work`), `cons/processes/palw_state_v2_sync.rs`,
  `cons/processes/palw_state_walk.rs`.
- [ ] Consensus modes (`PalwConsensusMode`: `Disabled`, `LegacyTn11`, `ConsensusV2`), and what each
  network selects. *Code:* `core/palw_mode_v2.rs`.
- [ ] Candidate-scoped state: which reads are taken at the candidate's own position. *Sources:* 0042 D5.
- [ ] The schema version of the state (v22 on testnet-12), and the rule that a version is set by
  fence and never by build. *Sources:* 0152 §6. *Code:* `palw_carriage_version_refusal_v1`.

## 2.2 Objects and their carriage

- [ ] One subnetwork id per object kind, and the kinds that exist today (registration, bond,
  attempt, receipt, licence, verdict and court objects, DA, lifecycle, market). *Sources:* 0046 D1,
  0029 (superseded shape). *Code:* `core/palw_carriage.rs`, `core/palw_lifecycle_objects_v2.rs`,
  `core/palw_fp_objects_v3.rs`.
- [ ] The two validation layers: stateless validity (in isolation) and validity against state
  (at acceptance). Say which errors belong to which layer. *Sources:* 0046 D2.
- [ ] A bond **is** its collateral output. The registration rides the output it locks. *Sources:*
  0046 D3, 0016. *Code:* `PalwBondKeyV2` (an outpoint).
- [ ] Panels are derived, never carried. Receipts and verdicts are carried, with their evidence.
  *Sources:* 0046 D4, 0108.
- [ ] Mass: every object fits the 480,000 standard mass cap, and oversize payloads are split by rule
  (for example the multi-transaction seed in 15). *Sources:* 0046 (mass budget), 0094.

## 2.3 Acceptance order

- [ ] Objects take effect in acceptance order: the selected chain in order, and each block's
  mergeset in GHOSTDAG order. *Sources:* 0046 D5.
- [ ] **Merged work is counted.** Claims carried by merged, non-chain blocks enter the state.
  *Sources:* 0058.
- [ ] From DAA 750 on testnet-12, a merging block applies a tied round lane **parents first**
  (`palw_lane_accept_parents_first`). Chapter 12 specifies it. This section states only the order it
  imposes.

## 2.4 What the header commits

- [ ] The state-root derivation and its hash ordering, with no challenge↔commitment cycle, and the
  None-root rule. *Sources:* 0043 §1–§4, and its version-17 amendment (0078). *Code:*
  `core/palw_block_commitment.rs`, `core/palw_state_chunk_map.rs`.
- [ ] What the root deliberately does not cover (0043 §3), and the frontier order used by the
  comparator (0043 §5).

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | from genesis. Parents-first round-lane acceptance from DAA 750 |

**Design:** `design/palw/state-and-carriage.md`.
