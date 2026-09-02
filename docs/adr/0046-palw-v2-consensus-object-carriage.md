# ADR-0046 — PALW V2 consensus-object carriage: the registrations ride their collateral, the verdicts ride their evidence

- **Status:** Accepted (2026-08-20)
- **Relates to:** ADR-0042 (the ruleset this feeds; its Decision 5 state machine consumes
  `PalwConsensusObjectV2`), ADR-0043 (the state root those objects move), ADR-0029 (the V1
  carriage whose Stage-1 shape this reuses), ADR-0009/0010 (the stake-bond rail this mirrors
  for bonds).
- **Lands with:** the acceptance seam (`consensus/src`) and the RC genesis loader. The wire
  types and stateless validation land now in `consensus/core/src/palw_carriage_v2.rs`,
  dormant — no shipped preset admits the band.

## Problem

`apply_palw_transition_v2` consumes a `Vec<PalwConsensusObjectV2>` per chain block, and nothing
defines where those objects come from. Ten variants exist; a network cannot run on the two the
genesis can seed. The V1 carriage (ADR-0029) answers the same question for the *evidence* lane
(commitments, attestations, openings, refutations); the V2 *state* lane needs its own answer,
and it is not the same answer, because three of the objects are not "signed statements someone
published" — they are UTXO events (bonds), derivations (panels), or the absence of an event
(defaults).

## Decision 1 — one subnetwork id per kind, band 0x50, Borsh body, no magic

ADR-0029 Stage 1, verbatim: the subnetwork id IS the address, the payload is the Borsh body of
one wire struct, bodies never embed their carriage, one object per transaction. The V2 state
band starts at 0x50 (0x46–0x4F stay reserved for V1 evidence growth):

```
0x50  SUBNETWORK_ID_PALW_V2_BOND          PalwBondCarriageV2         → BondRegistered
0x51  SUBNETWORK_ID_PALW_V2_RETIRE        PalwRetireCarriageV2       → BondRetireRequested
0x52  SUBNETWORK_ID_PALW_V2_BIND          PalwBindCarriageV2         → PanelBound
0x53  SUBNETWORK_ID_PALW_V2_RECEIPTS      PalwReceiptsCarriageV2     → ReceiptLicensed | ProducerDefaulted
0x54  SUBNETWORK_ID_PALW_V2_COURT_OPEN    PalwCourtOpenCarriageV2    → CourtOpened
0x55  SUBNETWORK_ID_PALW_V2_COURT_CLOSE   PalwCourtCloseCarriageV2   → CourtClosed
```

Four variants have **no wire kind**, deliberately:

- `ClassRegistered` / `ClassFrozen` / `ClassUnfrozen` — **genesis-only at RC.** BASE-0 is the
  only weight-bearing class (ADR-0042 Decision 8) and it is registered by the genesis loader,
  which verifies the class catalog against its committed root (`verify_against_catalog`) before
  producing the objects. A runtime class-lifecycle path is a governance decision; when a
  governance ADR wants one, it allocates its own kind rather than inheriting an unguarded one.
- The transition's **timeout edges** (`BindTimeout`, `ReceiptTimeout`, retirement completion,
  challenge-window expiry) — never carried. They are functions of the deadline index and
  `ctx.daa_score`, derived inside `apply_palw_transition_v2`; an object claiming "a timeout
  happened" would be a second source for a fact the state already knows.

## Decision 2 — two validation layers with two different failure meanings

The fork's own precedents, adopted as a pair:

- **Stateless shape** (decodability, version, sizes, signature lengths, internal signature
  checks that need no chain state) runs at transaction **isolation** validation. Failure is a
  transaction rule error — the `InvalidDnsOverlayPayload` shape
  (`tx_validation_in_isolation.rs`) — and a block carrying a malformed carrier is invalid.
  Malformation is always authored; there is nothing honest to protect.
- **Stateful admission** (does the bond exist, is the panel's anchor mature, does the quorum
  hold at this candidate chain point) runs at **acceptance**, against the candidate chain's
  own state. Failure means the carrier is **skipped — unaccepted, never block-invalidating** —
  exactly how a double-spend or a non-releasable stake-bond spend is treated today
  (`utxo_validation.rs`: "SKIPPED (not accepted, not muhashed)"). Two honest carriers can race
  (two retires for one bond, two binds for one claim); the second is stale by the first's
  effect, and a block builder must not be invalidated by mempool timing it cannot see.

The consequence that makes the walk total: **the object list a chain block hands to
`apply_palw_transition_v2` is the accepted-carrier list.** Acceptance filtered everything the
state machine would refuse, so a transition error on tx-carried objects is unreachable — the
only transition input that can still fail is the header-carried attempt, whose failure is the
block's own (ADR-0042 disqualification seam), not a carrier's.

## Decision 3 — the bond IS its collateral output

`SUBNETWORK_ID_PALW_V2_BOND` mirrors the stake-bond rail (`StakeBondPayload`, live on t10/t11):

- The carrier transaction's **output 0 is the collateral**. The bond key is
  `(carrier_txid, 0)` and `collateral` is **output 0's value, read, not declared** — a declared
  amount beside a real output is a second source that can lie
  (`BondOutputValueMismatch` is the V1 error this design makes unrepresentable).
- The payload carries the two keys and one signature:

```rust
PalwBondCarriageV2 {
    version:          u16,
    executor_pubkey:  Vec<u8>,   // ML-DSA-87, 2592 B — the key attempts are signed under
    operator_pubkey:  Vec<u8>,   // ML-DSA-87, 2592 B — the identity panels dedup on
    operator_signature: Vec<u8>, // ML-DSA-87 over H(domain ‖ executor_pubkey), operator's key
}
```

- **The operator co-signs the adoption of the executor key.** Without it, anyone could register
  bonds attributing executors to a victim operator (forced attribution: the victim's identity
  collects strangers' offenses). The signed message is the executor key alone — deliberately
  replayable, because the blessing is per-relationship, not per-bond: replaying it costs the
  replayer fresh collateral and gives the operator another bond it already consented to back.
- **Funding is the executor-side authorization**, as on the stake rail: no executor signature
  at registration, because a bond whose executor key the registrant does not hold is a bond
  that can never produce a valid attempt (every attempt is signed over `attempt_id` under that
  key) — it can only lose its own collateral.
- **Withdrawal discipline** (the spend-gate rule, landing with the acceptance seam):
  the bond outpoint is unspendable while its record is not fully retired; once retired, the
  release transaction may pay the owner at most `remaining = collateral − slashed`, and must
  carry one output of exactly the slashed total to the canonical PALW burn script
  (`PALW_V2_BOND_BURN_SPK`, provably unspendable, bytes pinned with the gate). The slashed
  part must not be expressible as fee — fee is miner revenue, and a producer who mines can
  otherwise slash itself into its own coinbase.

`SUBNETWORK_ID_PALW_V2_RETIRE` is the executor's own act:
`PalwRetireCarriageV2 { version, bond: TransactionOutpoint, executor_signature }`, the
signature over `H(domain ‖ bond outpoint)` under the bond record's executor key (checked at
acceptance, where the record is). Replay against an already-retiring bond is a stateful skip.

## Decision 4 — panels are derived, receipts are counted, courts carry their proof

- **`PALW_V2_BIND` carries `{claim, anchor}` and NOTHING else.** The seats are what
  `derive_panel_v2` computes from the candidate state and the anchor; acceptance derives them
  and hands the transition a `PanelBound` whose seat list is the derivation's. Carrying seats
  on the wire would be a second source for a pure function's output — the dual-source rule
  ADR-0029 already enforces for bodies. Binding is permissionless: the carrier needs no
  signature, because a correct bind is a fact about the chain, not a claim about the sender
  (`validate_panel_bound_v2` is the acceptance check; a wrong anchor or a dead claim is a
  skip).
- **`PALW_V2_RECEIPTS` carries `{claim, receipts}` and declares no direction.** Acceptance runs
  `validate_receipt_quorum_v2` (every receipt signature under the seat bond's registry key,
  `signed_daa` windows, `Unavailable` particulars, one answer per seat) and the quorum it
  returns IS the direction: `Licensed` maps to `ReceiptLicensed`, `ProducerUnavailable` to
  `ProducerDefaulted` — the two are provably disjoint (`2·quorum > seat_count`), so nothing is
  left for a declaration to add except a chance to contradict the derivation. Both objects hand
  the transition the seat-verdict set, because the transition prices the seats the quorum
  refuted (audit C5's both-directions slash).
- **`PALW_V2_COURT_OPEN` carries `{claim, challenger_bond, space, space_size,
  challenger_signature}`** — the signature over
  `H(domain ‖ claim ‖ challenger_bond ‖ space ‖ space_size)` under the challenger bond's
  executor key: a challenge names the stake that backs it (ADR-0042 Decision 8: refuted
  challengers are slashed) AND the dispute shape it commits to, so a relayer cannot re-mount
  one signature onto a different bisection space. `session_id` is **derived** — it is
  `court_session_id_v2(claim, claim.trace_root, executor_bond, challenger_bond, space,
  space_size)`, whose extra inputs come from the candidate state, which is why the carrier does
  not carry it (`validate_court_opened_v2` is the acceptance check; a lapsed window, a missing
  challenger or a self-challenge is a skip).
- **`PALW_V2_COURT_CLOSE` carries `{session_id, proof}` and NO verdict.**
  `adjudicate_court_close_v2` returns the only verdict the proof supports (the terminal
  arithmetic adjudication of ADR-0030/0033, C3's `check_execution_root_binding` included), and
  the transition is handed `CourtClosed` with THAT verdict — never one the object announced. A
  proof that does not adjudicate is a skip. Timeout defaults are NOT carried (Decision 1) — a
  session nobody closes is closed by the deadline sweep.

## Decision 5 — order is acceptance order

The object list of a chain block is the concatenation of its accepted carriers' objects in
**canonical acceptance order**: mergeset order, then intra-block transaction index — the exact
sequence acceptance data already fixes for every node. No PALW-specific ordering exists; a
second ordering would be a second consensus.

The header-carried attempt is **not part of this list** — it is the transition's separate
`attempt` argument, applied after the block's objects (registrations in a block admit attempts
in that block's own children at the earliest, since admission runs against the parent-side
state per ADR-0043).

## Mass budget (against the 480,000 standard cap, mass_per_tx_byte = 1)

| carrier | dominant terms | est. bytes | fits |
| --- | --- | --- | --- |
| bond | 2 × 2592 pk + 4627 sig | ≈ 9.9 K | ✓ |
| retire | 4627 sig + outpoint | ≈ 4.8 K | ✓ |
| bind | two hashes | ≈ 0.2 K | ✓ |
| receipts (5 seats) | 5 × (4627 sig + verdict) | ≈ 24 K | ✓ |
| court open | 4627 sig + hashes | ≈ 4.9 K | ✓ |
| court close | terminal one-step proof | court-capped (≪ ADR-0029's 152 K answer bound) | ✓ |

## What this ADR does not decide

- The **bytes of `PALW_V2_BOND_BURN_SPK`** and the spend gate's wiring — they land with the
  acceptance seam, which owns UTXO rules.
- The **court proof's future proof classes** — `PalwCourtVerdictProofV2` is the court module's
  own enum (today: `Arithmetic { refutation, operand_openings }`); the CLOSE carrier presents
  whatever variants that enum grows, and acceptance verifies them through
  `adjudicate_court_close_v2`. This ADR fixes the carriage, not the proof taxonomy.
- Any **runtime class lifecycle** — a governance ADR allocates its own kinds.
- **Relay/mempool policy** for the band (standardness, RBF-adjacent races) — policy, not
  consensus; the skip semantics above make every policy outcome safe.

## Number hygiene

This record briefly shipped as ADR-0045 and was renumbered the same day, by its own rule.
0043 was claimed hours earlier by a parallel session (`palw-v2-state-root-ordering`), 0044 is
committed on the `palw-freeprompt-v3` branch (free-prompt PALW), and 0045 — checked against
committed branches at numbering time, then claimed at 17:56 by a THIRD parallel lane
(`palw-cross-class-v1`, `palw-class-economy-on-chain`) nineteen minutes before this record's
first commit at 18:15. ADR-0036 Decision 5 breaks the tie by timestamp: the later writer
renumbers, and that is this record — now 0046. (Two commits on this branch, `aebb5b24` and
`c1425406`, say "ADR-0045" in their immutable messages; they mean this record.)
