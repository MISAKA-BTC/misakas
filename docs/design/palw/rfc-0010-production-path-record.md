# RFC-0010 permissionless Panel — production-path record (C2)

Branch `rfc10/c2-panel`, 2026-10-08. Companion to `docs/rfc/0010-dormant-implementation.md` (the reference engine) and
`remaining-rfc-integration-matrix.md` (§1 C2 rows, §2 allocation). This record says what the production fold does, what the
beacon rule is, and — quantitatively — what that rule does **not** buy. It is not a security proof and does not activate anything.

**Status, precisely.** The V3 engine is now a Some-only sub-state of `PalwChainStateV2` with a fold, journal, carriage and
V2 handoff (**IMPLEMENTED_AND_TESTED at the consensus-core fold**, processor/IBD drills below). `palw_permissionless_panel_v1`
**stays refused at every real height**: the beacon is `BEACON_UNAVAILABLE` (no scheme approved, no Panel-independent Final, no
G14-complete profile set) and its bias/withholding/P0-10 review is external. **RFC-0010 is not complete.**

## 1. What is implemented

| Item | Value |
|---|---|
| State | `PalwChainStateV2.panel_v3: Option<PermissionlessPanelStateV1>`; `None` below the fence |
| Root | one Some-only block `panel_v3/v1` after `mesh/v1`, content = the engine's own root (policy, cursor, claims, retained work ids, certified outputs, reservations) |
| Delta | explicit `170 PanelV3Cursor`, `171 PanelV3Claim`, `172 PanelV3WorkId`, `173 PanelV3Beacon`; cursor first, so revert removes rows before the cursor; per-entry `DeltaMismatch` checks; reservations re-derived after a delta |
| Carriage | tail `0xED` (the engine, written only when `Some`) |
| Object | tag `120 PanelBeaconProofV3` — evidence, never a command; a proof that does not verify is dropped and the block stands |
| Void reasons | `SealUnavailable = 120`, `BeaconUnavailable = 121`, `PermissionlessNoCapablePanel = 122`; `PanelUnavailable (10)` reused for retry exhaustion. `PalwVoidReasonV2` gained `#[borsh(use_discriminant = true)] #[repr(u8)]`; 0–10 are unchanged |
| Fence | `palw_permissionless_panel_v1`, mirrored on the bundle's state params (`PalwStateParamsV2::panel_v3`, borsh-skipped, set by `Params::sync_palw_permissionless_panel_v1`); `validate_palw_permissionless_panel_v1` refuses every real height |
| Engine API | `advance` / `accept_beacon` / `admit` stages (`fold` ≡ their composition, pinned), keyed cursor + rows, `ConsensusViewV1::receipt_clock` |

**Fold order** (`apply_palw_transition_v7`; `palw_v2_pre_object_base_v1` mirrors 2f):

```text
2f  advance_v1      releases (V2 left the Panel) → seal against the PARENT checkpoint → readiness / BeaconUnavailable →
                    due draws (first draws and receipt-timeout redraws) against the pre-object one-ledger headroom → V2 bind / V2 void
3   objects         a certified output (tag 120) is queued
4b″ take_block_v1   accept_beacon per queued output (failures dropped) → admit the claims this block created under the V3 rule,
                    in V2 acceptance order (journal order of Claim{old: None}) → engine journal (170–173)
```

* **Rule by acceptance.** `panel_v3_rule_at(claim.accepted_daa)` — never the binder's or a retry's height. A claim accepted below
  the fence is lane A's for life; it is not in the engine and drains under its original anchor, deadline and court rules.
* **The engine is the clock.** A claim the engine tracks owes no V2 bind or receipt deadline (`palw_rcore_deadline_v1`, the
  shape check and the DA re-arm skip it); the lane-A binder list (`palw_claims_provisional_past_their_anchor_slot_v1`), step 4c
  and the retry sets never offer it; a lane-A `PanelBound` object for it is refused by the processor's acceptance walk.
  An S2 licence no supplementary set raised to a replay-backed one **expires** (`PanelUnavailable`, uncharged) instead of being
  redrawn by a V2 binder that does not exist for it.
* **One exposure ledger.** A binding writes the V2 `PalwPanelStateV2` (anchor = the V3 seed), the duty row and each seat's
  `reserved_exposure` through the single writer `reserve_seat_duties_with` (factored out of V2's `reserve_seat_duties`); the
  engine's reservation map mirrors those rows. The engine draws against V2's `gate_room(.., Work)` plus its own live duties
  added back, so a seat is never counted twice and a lane-A duty and a V3 duty cannot spend one sompi (both directions tested).
  The per-seat price is the eligibility `max(duty_bind, lock_2)` at admission; V2 would reserve `duty_bind`.
* **Receipt/court handoff.** The bound claim is an ordinary V2 `PanelBound` claim: seats sign ordinary receipts, an ordinary
  quorum licenses it, `Valid` signers hold slashable locks, a non-seat public bond opens a DA session and the default convicts
  the producer (`ProducerWithholding`), courts open on it. Before the bind it has no panel and is `DaClaimNotAccusable`, as V2.
* **Non-fraud ends** (`SealUnavailable`, `BeaconUnavailable`, `NoCapablePanel`, `PanelUnavailable`): V2 void under the versioned
  reason, no slash, no strike, no E-4 hold, the reservation and every duty released at once; the fee is spent and the seats
  are paid nothing. Tested for each.
* **The receipt clock never runs against a pending accusation (G14, structural).** It is paused while ANY DA session (seat or
  non-seat — V2 itself pauses only for seat sessions, V3S-08) or court session is open on the claim, and a session's close
  re-bases the window to start no earlier than that close. Without it a colluding producer + Panel could let the V3 window lapse
  and void the claim uncharged, closing the session neutrally. Test: a non-seat accusation is open when the window (3 DAA, one
  redraw) would have expired the claim `PanelUnavailable`; the outcome is the DA default, `ProducerWithholding`.

## 2. The production beacon rule

`BeaconProofV1.proof` = borsh `WorkBeaconV1` of subject kind `PANEL_ASSIGNMENT`, verified with `verify_work_beacon_v1` against the
branch's own settlements, read at the carrying block's selected parent (`palw_panel_beacon_v1`). Positions are DAA scores; the
epoch's subject is committed at `release_daa` (every claim sealed to it was sealed strictly before), so `S = release + anchor_delay`.

Refused by construction (each a test in `rfc0010_beacon_adapter.rs` / `rfc0010_production_fold.rs`): a raw future block hash, a
heartbeat, BASE-0, EXEC (tx or slice), receipt-only, provisional or bare Panel-receipt contribution, a signature, a candidate-chosen
nonce as the *claim*'s entropy, the claims under test, a stale (pre-`S`), late, non-final, DA-unsatisfied, dependent, duplicate,
reordered or substituted contribution, a forged output/accumulator/anchor, another epoch's proof, oversize or malformed bytes,
any DNS/BFT/operator/test-certificate source. **Never work → Panel → Final → beacon → Panel:** `FinalPathV1::PanelLicensed` is
refused for this subject.

**Why it is `BEACON_UNAVAILABLE` today — three independent locks**, any one sufficient:

1. `approved_panel_beacon_policies_v1()` is empty (EXTERNAL_GATE_PENDING: no scheme is approved);
2. every Final the V2 lattice writes is Panel-licensed (a claim reaches `Final` only from `ReceiptLicensed`), so
   `palw_panel_v3_final_events_v1` yields `PanelLicensed` events only;
3. no class is G14-complete in consensus (the code-derived `PUBLIC_PROSECUTION_COMPLETE` gate is not linked), so the chain's
   eligible-profile set is empty.

Consequence: every V3 claim ends `BeaconUnavailable` (non-fraud) on a real chain. The fold's verification path is exercised only
through `PalwPanelV3BeaconSourceV1::Reference` (a fixed history), which no processor resolves.

## 3. Quantitative note on the source rule (EXTERNAL_GATE_PENDING — a model, not a proof)

Symbols: `k` sources mixed, `D` settlement depth, `f` the adversary's share of eligible settling sources, `s` its share of the
drawn seat weight, `p0(s)` = P(≥ 3 of 5 seats adversarial) for the 5-seat / quorum-3 panel (`p0(0.05)=1.2e-3`, `p0(0.10)=8.6e-3`,
`p0(0.20)=5.8e-2`, `p0(0.33)=2.1e-1`). All figures are upper bounds under the stated model.

**Reorg.** The lock is branch-relative (k-th settlement + `D`), not finality. A Nakamoto-style bound for an adversary with
fraction `q` of chain weight reversing a `D`-deep lock is `(q/(1−q))^D`: `q=0.10`: `2.9e-10` (D=10), `2.4e-29` (D=30);
`q=0.25`: `1.7e-5`, `4.9e-15`; `q=0.40`: `1.7e-2`, `5.2e-6`, `2.7e-11` (D=60). Reversing the lock also reverses the source's own
settlement, which lies at least `D` deeper. **Gate:** `D` must be chosen so the bound at the network's credible `q` is below the
Panel-capture value; and a reorg re-derives (rolls back) every dependent binding — tested by the delta revert at every block and
the carriage reload; the engine's journal is branch-relative.

**Withholding / inclusion choice.** A controlled source may include or exclude itself (decline to submit a claim, or let it lapse):
one bit per controlled *late* source, so `2^m` selectable outputs for `m` controlled late sources. `P(m ≥ j)` for `k` mixed sources:

| `f` | `k` | P(m≥2) | P(m≥4) | P(m≥8) | m at 99 % → tries `2^m` |
|---|---|---|---|---|---|
| 5 % | 16 / 32 / 64 | 0.19 / 0.48 / 0.84 | 0.007 / 0.074 / 0.40 | 3.5e-7 / 1.4e-4 / 1.4e-2 | 4→16 / 6→64 / 9→512 |
| 10 % | 16 / 32 / 64 | 0.49 / 0.84 / 0.99 | 0.068 / 0.40 / 0.89 | 6e-5 / 1.2e-2 / 0.31 | 6→64 / 9→512 / 13→8192 |
| 25 % | 16 / 32 / 64 | 0.94 / 1.00 / 1.00 | 0.60 / 0.98 / 1.00 | 0.027 / 0.57 / 1.00 | 9→512 / 15→32768 / 25→3.4e7 |

This is a *ceiling* (it assumes every controlled source sees all others before choosing). Increasing `k` does **not** help — it
raises the expected `m` for a fixed `f` — so `k` is a liveness knob, and the bias is bounded by the share `f` of eligible
sources the adversary controls, i.e. by the *cost of being an eligible, Panel-independent, G14-complete source* (not by `k`).

**Last mover / candidate grinding (P0-10).** The mixed item is `(profile, canonical_work_id, execution_commitment)`; a source
producer can vary its job (so its `canonical_work_id`/commitment) at the cost of one useful-work execution per try `c_exec`.
Tries to capture a quorum: `ln(1−c)/ln(1−p0)`: `s=0.10` → 81 (50 %) / 349 (95 %); `s=0.20` → 12 / 50; `s=0.33` → 3 / 13. So the
grinding cost is `≈ T · c_exec` with `T` from this table, and the bias is acceptable only if **`min c_exec` over the eligible
profiles ≥ `V_capture / T`**, where `V_capture` is the most a captured Panel can extract. Therefore (a) BASE-0 and heartbeat are
excluded (their `c_exec` is ~279 junk BLAKE2b draws — the P0-10 figure the lane-A comment records), (b) the eligible set must
exclude cheap profiles, (c) the gate is quantitative, not structural, and **is not met by anything in this tree**.

**Retry bias.** The redraw is deterministic from the original seed and non-reused operators, so a silent colluding seat chooses
between `R+1` publicly known panels (`R = max_retries`): `1−(1−p0)^(R+1)`; `R=1`, `s=0.20`: `0.113` against `0.058`. Retries are a
bias multiplier of at most `R+1`; the policy keeps `R` small.

**Free abandon / withdrawal after observation.** `SealUnavailable`/`BeaconUnavailable`/`NoCapablePanel` end a claim before any
Panel is observable, so they give no observation advantage. `PanelUnavailable` ends it *after* the Panel is public, uncharged, and
a colluding seat can force it by silence; the claim's work is then lost to the producer but nothing is slashed. Mitigations in
this tree: the retry set is deterministic (no fresh randomness for the same claim), the clock is paused under any pending
accusation, and a new claim is a new seal (a new draw) priced at one execution plus the fee. Open: whether `PanelUnavailable`
should keep V2's E-4 obligation hold for V3 claims so a re-roll costs capital-time (a policy decision for the review).

**What bias does not buy under G14.** A captured Panel licenses faster or later; it does not make fraud final, because one
ordinary public bond outside the Panel still reaches objective conviction or DA default from public material (tested). The
bounds above price *liveness and payout speed* and the cost of a free-prompt-style re-roll, not safety — provided the claim stays
accusable for its whole life and no non-fraud end pre-empts a pending accusation (the structural pause).

**P0-10.** The anchor re-roll of the lane-A lattice (`~279` junk draws on the testnet-12 floor) does not exist for V3: the seed is
`H(seal, anchor slot, snapshot root, epoch, certified output)` with the seal fixed by the accepted claim and no anchor *block*
in it. What replaces it is the table above.

## 4. G14 properties exercised (rfc0010_production_fold.rs, through testnet-12's own fold)

* a V3-bound claim is accused and defaulted by a non-seat public bond exactly as a V2 claim (`ProducerWithholding`, producer slashed,
  no seat charged); before the bind it is `DaClaimNotAccusable`;
* the accusation outlives the engine's receipt window (structural pause);
* a V3 duty and a lane-A duty cannot spend the same collateral, in both orders;
* V3 seats sign ordinary V2 receipts and hold slashable locks; the claim finalises by V2's sweep and every duty is released;
* every block's delta re-applies and reverts, and the carriage reloads under its root.
* a bond registered after genesis (the ordinary `BondRegistered`, no named operator) is in the sealed snapshot and is seated when
  the seat count equals the population (`a_bond_registered_after_genesis_…`; folded without the acceptance layer's ML-DSA check,
  like every test here — a real mature non-genesis cohort is the external drill below);
* a snapshot whose candidates, root, policy id or excluded producer was touched neither reloads under the committed state root nor
  passes the engine's own check (`a_tampered_snapshot_…`); in the fold the snapshot is host-derived, a producer carries none;
* the read model (below) follows a claim from acceptance to release and names every non-fraud end with `fraud: false`.

## 4b. Through the real virtual processor (testnet-12 harness, `t12_permissionless_panel_e2e.rs`)

**These tests bypass `validate_palw_v2`, and say so.** `ConfigBuilder::build` panics at an armed `palw_permissionless_panel_v1`
(as it must on a real network), so the fence is set on `Config.params` after `build` and mirrored by
`sync_palw_permissionless_panel_v1`; the processor takes its beacon source from a `#[cfg(test)]`-only override
(`VirtualStateProcessor::panel_v3_test_beacon`, compiled out of every non-test build) because no Panel-independent Final exists on a
real chain. Nothing here is evidence that the permissionless Panel is complete.

| Test | What it pins |
|---|---|
| `t12_a_v3_claim_binds_on_a_heartbeat_with_no_named_operator_while_lane_a_drains_the_legacy_claim` | a legacy claim (below the fence) is bound by lane A's own binder; the V3 claim is bound by the engine on a **heartbeat** block — no named operator, no anchor attempt — in its acceptance; a lane-A `PanelBound` of the V3 claim is refused at the gate; the duty row, the exposure and the V2 panel anchor (= the V3 seed) agree; the node's read (op 220) equals the pure function of the tip state |
| `t12_a_node_that_syncs_the_chain_reaches_the_same_engine_state_block_by_block` | a second node fed A's blocks (IBD order) has A's PALW state root after every block; the tip carriage reloads under it |
| `t12_the_bind_survives_a_reorg_and_the_carrier_does_not_change_the_panel` | A and A2 bind on different carriers: same seed, same seats, same exposure, different `binding_block` (the witness only); a third node follows PALW fork choice A → A2 → A and its state is the one the branch it stands on commits, at every switch |
| `t12_a_certified_output_carried_below_the_fence_is_dropped_by_name_and_the_block_stands` | the gate names the fence; a block that carries tag 120 below it stands, no engine appears |

PALW fork choice orders by safe frontier, safe weight and live total (safe + bounded immature), not by blue work, so the reorg
test makes the branch with more live attempt work win and says so; it does not assert blue-work order.

## 4c. The observation read (RPC op 220, `misaka palw panel-v3`)

`getPalwPanelV3Status` (wRPC op **220**, the first of the lane-C2 range 220–229; gRPC `KaspadRequest/Response` fields **1254/1255**,
the `2·op + 814` series ops 199–201 follow) returns the node's `PanelV3ObservationV1` as one camelCase JSON document with its own
`version` (fields are only appended): the engine's overview (tip, cursor, per-phase counts, certified epochs) and, per claim, the rule
(`permissionlessV3` / `historicalLaneA`, decided by acceptance), V2 phase, engine phase, seal, frozen snapshot summary, the epoch's
certified-output state (`collecting` / `certified` / `unavailable`), the assignment (retry index, seed, beacon id, seats, exposure,
bound DAA, witness block), the redraw count and the terminal reason (`SEAL_UNAVAILABLE`, `BEACON_UNAVAILABLE`, `NO_CAPABLE_PANEL`,
`PANEL_UNAVAILABLE`, all `fraud: false`, or `RELEASED`). Request: up to 64 claim ids (128 hex each, a malformed id is an error before
any state is read) or none for the first tracked claims; unknown ids are listed, never silently absent. A pure read of the committed
tip: no rule calls it. JSON in a string rather than a typed protobuf tree is deliberate: it is a read model, and a typed mirror can
follow when an explorer needs one. A node built before op 220 drops the WebSocket on it (like every tail-appended op): ask it on a
connection of its own. `misaka palw panel-v3 [--claim ID]… [--limit N] [--json]` prints it.

## 5. Open items (not hidden)

| Item | Status |
|---|---|
| Beacon scheme approval, bias/withholding/last-mover/P0-10 review, `k`/`D`/delay/window numbers | EXTERNAL_GATE_PENDING |
| Panel-independent Final path (RFC-0014 public window lapse / RFC-0015) | EXTERNAL_GATE_PENDING (A) |
| G14-complete profile set linked into consensus | EXTERNAL_GATE_PENDING |
| Objective L1 seal/finality rule (seal depth is not a finality primitive) | DESIGN_GAP |
| Per-shard V3 draw: a claim of a class with a shard plan ends `PermissionlessNoCapablePanel` (a flat panel cannot license by parts) | IMPLEMENTED_AND_TESTED at the fold, dormant (agent SHARD, `shard-rfc6-10.md` §1: the engine's strata, the per-shard record written by the V3 bind; `rfc0010_shard_v3.rs`); the beacon gate is every V3 draw's |
| State growth: terminal engine records and retained work ids are never compacted (`max_tracked_claims` fills for good) | DESIGN_GAP |
| Drain bound for lane-A claims (their original timeout / court / DA liability) | DESIGN_GAP (time) |
| `receipt_window_daa`, `max_retries`, `seal_wait_daa` are policy numbers with no network default | EXTERNAL_GATE_PENDING |
| `PanelUnavailable` obligation hold for V3 (re-roll cost) | policy decision |
| Real-node IBD / non-genesis cohort drill (a mature bonded cohort that is not the genesis operators, on a multi-node network) | EXTERNAL_GATE_PENDING |
| Sharded-IR classes under V3 | IMPLEMENTED_AND_TESTED at the fold, dormant (the per-shard V3 draw above); needs the beacon |
| G14 pre-emption by a V3 non-fraud end (S2 expiry, pre-bind seal/beacon end) | IMPLEMENTED_AND_TESTED at the fold, dormant (agent SHARD, `shard-rfc6-10.md` §2: `palw_accusation_pending_v1`, the deferred end, DL-1's G14 row; `rfc0010_g14_guard.rs`); lane PL part C (`palw_panel_unavailable_expiry`) on a V2 claim keeps V3S-08 — a fence decision for the full-activation release |
| Processor-level `PanelUnavailable` / `NoCapablePanel` and exhausted-alternates cases | fold-level only (`rfc0010_production_fold.rs`); the processor tests cover bind, IBD, reorg, legacy drain, the fence drop |
| An RPC typed protobuf tree for the observation (explorers) | not built; the JSON document is versioned |

## 6. RFC-0006 gaps handled with this lane (what changed, what is proposed)

* **Shard reorg + duplicate receipt — fold level, DONE** (`palw_tir_shard_fold.rs::shard_parts_are_branch_local_…`): from one bound
  claim, branch A lands shard 0's part then shard 1's, branch B the other order; both license with the same `basis_k`; A's deltas
  revert to the bound state exactly (parts, per-shard progress, cell counts and every seat lock), the very receipts A spent fold
  again on B (a receipt is spent per branch, never globally), B's deltas reproduce B's states, and inside one branch the same part
  twice is refused by name (`ShardAlreadyLicensed`). **Processor/T12Chain-level twin: GAP** — it needs an IR class with a shard
  plan and a signed `ReceiptV4` chain through the real node, which the T12 harness does not yet build; D-S1…D-S6 drill evidence is
  EXTERNAL_GATE_PENDING.
* **Stale "dormant" wording — DONE** (RFC-0006 status note, `palw_tir_shard_v1.rs`, `palw_tir_shard_fold_v1.rs`, `config/params.rs`):
  `palw_tir_shard_v1` is armed on testnet-12 at DAA 5,300.
* **Per-segment resource pricing — DECLARED DORMANT by agent SHARD** (`palw_tir_shard_segment_v2`, refused when armed; `shard-rfc6-10.md`
  §4: `max(work, resident)` for the lock and the pay, keyed on the claim's acceptance; readiness = the shard's heaviest cell, no on-chain
  tier). The original proposal, kept for the record: The armed rules already price a cell's *work*: the plan's cell
  shares come from `palw_tir_shard_cell_permille_v1` (`work_cell_v1(layers, positions)`, so attention work grows with the
  segment's positions) and a signer's lock is `full_lock × share(mask)`. What they do not price is *residency*: readiness is one
  possession proof per **shard** (`palw_tir_shard_ready_class_v1`), while a partial seat's minimum state is the whole K/V prefix up
  to its segment's end (`palw_resource_profile_v1`: `end_rows`, `kv_resident_bytes`), which grows with the segment index. A seat can
  therefore (expected from the profile; not drilled) hold shard `i`'s rows, be drawn into a late cell it cannot host, and be a non-responder there (a liveness cost borne by
  the claim, not a safety hole: the exact court and `basis_k` are unchanged). Any fix changes the draw and the price, so it needs a
  **new versioned fence** (suggested name `palw_tir_shard_segment_v2`, undeclared and unarmed) with: a per-`(shard, segment)`
  resident-byte table in the plan; a readiness tier a seat proves (the highest segment it can host); the stratified draw
  restricted to seats whose tier covers the cell; and the lock share `max(work share, resident share)`. The armed fence at 5,300
  keeps its rules. Needs a decision on the tier granularity and on whether a seat that over-declares a tier is slashable (open; not checked against
  the shard readiness court rules in this lane).
* **Non-seat public cell watcher — IMPLEMENTED by agent SHARD** (`--palw-tir-shard-watch`; `palw_tir_shard_watch_v1` targets,
  `kaspad/src/palw_panel/tir_shard_watch.rs`; `shard-rfc6-10.md` §3). The original gap, for the record: `TirShardCourtAccused` is already open to any Active bond, but nothing in the tree plays
  a non-seat watcher: it needs the public material read for outsiders (matrix row "A row 16": carry-in tiles, history rows,
  checkpoints) and A's fresh-verifier engine to replay one cell. No code in this lane.
* **V3 per-shard draw — IMPLEMENTED at the fold by agent SHARD** (dormant with the fence; `shard-rfc6-10.md` §1). The original gap: Blocked on the beacon (BEACON_UNAVAILABLE) as the matrix says; until then a sharded class
  never reaches a Panel under V3.
