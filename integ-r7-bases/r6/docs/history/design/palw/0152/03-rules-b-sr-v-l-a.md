> **Archived verbatim from ADR-0152 v3.1** (branch `docs/adr-0152-v31-postedits` at `9ed1adced`,
> lines 749–1486 of `docs/adr/0152-account-stake-staged-reserve-and-vested-rewards.md`): §3.1 B (bond), §3.2 SR (staged reservation), §3.3 V (vesting rows, H-1), §3.4 L (seat lock), §3.5 A (one ledger).
> Not normative. The rules are in [spec/palw](../../../../spec/palw/00-index.md); the decision record is
> [ADR-0152](../../../../adr/0152-account-stake-staged-reserve-and-vested-rewards.md); the reading guide is [README.md](README.md).

## 3. Normative rules

### 3.1 B — the bond

**B-1 (one account).** Owner A (S). Tests T09, T22. A bond's collateral is one account of slashable stake. It is
debited on the one committed ledger (A-1) against `collateral × 500‰`. The uncommitted half funds the action tiers,
accuser exposure (A-6) and the reporter reward (§3.6 R).

**B-2 (floors).** Owner A (S). Test T17. Unchanged from v2.
* The producer and registration floor is 13,000 MSK (`min_collateral_sompi`). The seat draw floor is
  130,000 MSK (`palw_panel_economy_v1.rs:70`).
* Past this fence `palw_bond_registration_floor_v1` returns `min_collateral_sompi`, explicitly reversing
  `139c9215`'s panel-floor rule (#12 "c-floor"); the audit session agreed.
* #12's burned rent and per-block cap apply to `ClassRegistered` only. No `BondRegistered` per-block cap is added (§9.1 Q7, decided).

**B-3 (exit stays bounded; load-bearing; F19).** Owner A (S). Tests T23, T42.
* Past this fence the withdrawal gate (`palw_bond_collateral_is_locked_v6`, replacing
  `palw_bond_collateral_is_locked_v5` / `palw_bond_backs_live_duty_v2`) holds while **any** of these holds:
  * `palw_bond_committed_v1(bond) − registration_exposure(bond) > 0` — own claims' commitments (including a
    free-prompt claim on its abandon hold, SR-1) and every `max(duty, live lock)` term (A-1). This follows duties out
    of `reserved_exposure`; today's gate reads `reserved_exposure(key) > 0` (STATE:2057 at `e93be0f2`), which A-1
    empties of duties. **Registration exposure is not in the gate** (IMPL-8): it is released only when the class
    goes dormant, freezes or is reclaimed, never at retirement, so including it would pin a registrant for as long
    as its class lives. Today's v5 gate does not read it either; it stays inside the 500‰ ceiling (A-1);
  * `palw_accuser_exposure_v1(bond) > 0` — open DA sessions, refuted exposure held for its claim, and court
    challenger reservations (A-6, IMPL-4). Otherwise a refuted accuser could withdraw before DA-6's charge lands;
  * the bond is payee of a vesting row that is **unmatured by V-4(a)**, the lock predicate including the escape.
* It does **not** hold for presence in the table, for the halt (V-4(b)), for an open DA session on the row
  (V-4(c)), or for a carried row. So a licence halt never freezes exit.
* **Exit bound:** `max(since + 12,900, last Final of the bond + 9,000)`; courts and DA sessions gating Final can
  push the last Final. **The 12,900 includes the DA lattice (post-edit 10(d)):** it is
  `palw_v2_bond_withdrawal_delay_at_v1(bundle, palw_da_court, 0)` (`params.rs` at `f8c91f19`, S-SPEC §1e), the
  withdrawal delay 7,500 plus the DA lattice 5,400, which is the value the processor uses, **not**
  `bundle.bond.withdrawal_delay_daa()`. S-SPEC pins it as a function in T23: with the second clock stalled the hard
  bound is that delay `+ 2 · window_court` = 18,900; 12,900 when the licence clock advances; `F + 9,000` for locks. Without B-3 a producer could withdraw before its S3 conviction lands (horizon 10,347 DAA
  with no court, about 13,347 with one court; 2M 14,740).
* A retiring seat that is bound on a panel but has not signed now fails the gate (T23's retire-while-bound case).
* A registrant whose class lives exits by the same bound (T23's registrant case).

**B-4 (no ejection status, no tombstone; a producer floor gate; Eq capped on t12; X12, D6, U2).** Owner A (S; S-SPEC's
commits S-1 for the symbols and S-3 for the gate; not the addendum's seat fixes, which this ADR calls SEAT-S1…SEAT-S4),
B (P6). Tests T36, T76, T17.
* No `Ejected` status and no `operator_id` tombstone are added. Every per-bond rule here is **capital pricing, not an
  identity limit**. (v3.1 correction, SW-A3: on t12 `palw_operator_id_unique` is armed at genesis, so an operator
  identity backs one bond, ever (`DuplicateOperator`, STATE:17275-17280 at `0533e1de`), and a registration proves
  possession of its operator key (PROC:7835-7853). v2's "`operator_pubkey` is never signature-verified" holds only
  below that fence. A fresh key still costs nothing, so a second identity costs a second bond at the floor, which is
  capital again.) v3.1's panel draw follows the same principle: seats are priced in posted stake, not in operator ids
  (§3.14).
* **The producer floor gate (U2, post-edit 12; amends v3's B-4).** "Ejection" is the capital predicates (the seat floor,
  the 500‰ ceiling, the accuser and reporter floors, the bounded exit) **plus** a producer floor gate: producing
  requires the bond's **current posted collateral** (after slashes; not its free stake) ≥ `min_collateral_sompi`
  (13,000 MSK on t12).
  * **Symbols (S-1, STATE):** `palw_bond_producer_floor_shortfall_v1(state, params, bond, now_daa) -> Option<u64>`
    (`None` = meets the floor, or dormant; an unknown bond gives `Some(floor)`) and
    `palw_bond_meets_producer_floor_v1(..) -> bool`. Dormant unless `palw_rcore_plus` is active at `now_daa`, read
    through the `rcore_plus_from_daa` mirror (§6).
  * **Where it is checked (S-3):** at attempt admission, in ADM and in the fold's `apply_attempt`, where the block's
    own attempt is refused with the **non-fatal** `PalwStateV2Error::ProducerBelowFloor { bond, collateral, floor }`,
    like `AttemptExposureCeiling` (the attempt is skipped; the block stands); and for FP executors at commitment.
  * **kaspad reads the same predicate (P6, B):** the producer pre-check at `kaspad/src/palw_panel.rs:2438` switches to
    `palw_producer_facts_v4(.., raw_depth).committed` and this gate, so a producer never mines an attempt the fold
    refuses, and logs "holding: top up <shortfall> sompi to reach the producer floor". **P6 is a hard precondition for
    any drill of an S-bearing build** (§8.3). Until §9.3 Q11 is decided, "top up" in that line means the
    re-registration below; P6 should say so in the log's help text rather than imply an in-place deposit.
  * **As integrated (IA-15; `42ef9642`, `0613580f`):** there is **one readiness verdict** for the node and the RPC,
    `PalwProducerFactsV2::ready_to_produce_v3(key, rcore_plus)`. Below the fence it is `ready_to_produce` byte for
    byte. Past the fence it checks, in `apply_attempt`'s order: the key and the bond, the producer floor
    (`PALW_NOT_READY_BELOW_PRODUCER_FLOOR_V2`), the class gate, the epoch budget, and the committed room
    (`has_committed_room`, A-6's work gate, IA-2). kaspad's `palw_producer_ready_v1` adds only the floor hold's
    sentence, which names the shortfall and the way out (a new bond at the floor under a new key), and, last, the
    SW-10 stake question. That question is not a refusal of the attempt; its consensus read is pending, so it answers
    "unknown" (`palw_class_eligible_stake_at_floor_v1`). `getPalwProducerFacts` reads the same verdict, and its
    wire v9 carries `bond_committed`, `bond_producer_floor_shortfall` and `bond_accuser_exposure` (gRPC tags 31–33).
    The FP price (`palw_fp_commitment_price_impl`) answers the fold's `ProducerBelowFloor` first, then the T-2(a)
    share refusal, in the FP arm's order. The fold's own-attempt skip arm is `AttemptExposureCeiling |
    ProducerBelowFloor | BondClassShareExceeded`.
  * **Accepted consequence:** an honest 13,000 MSK producer that takes one S0′ (−3,200.95 on the floor) is below the
    floor and cannot produce until its operator "tops up". There is still no status and no tombstone. (S-SPEC's own
    default was "no producer floor gate"; the operator decided otherwise, S-SPEC §10.)
  * **What "top up" is (open, §9.3 Q11).** U2, post-edit 12 and S-SPEC §10 say "tops up" and "a top-up restores
    production" but define no mechanism, and the code has none: the registry is append-only (`write_bond(_, None)` is
    reachable only from a delta undo, STATE:7580 at `f1dfb33b`); `BondRegistered` refuses an existing bond key
    (`DuplicateBond`, STATE:17236), a bond public key that has ever held a bond (`DuplicateBondKey`, STATE:17261) and,
    with `palw_operator_id_unique` armed at t12 genesis, an operator identity that has ever held one
    (`DuplicateOperator`, STATE:17278); and the only non-test write that changes `collateral` is the slash debit
    (STATE:13620). **So the one route the code gives today is re-registration:** a new bond under a new bond key and a
    new operator identity, posting ≥ 13,000 MSK, passes the gate once its registration is folded (the other
    predicates apply to it as to any new bond), and its seats count only for claims anchored after its registration
    (ADR-0147's cut, SW-8); the old bond sends `BondRetireRequested` and exits by B-3's bound. An in-place top-up
    would be a new consensus object with its own fence, ledger and draw effects; if one is chosen, SW-2's weight and
    SW-8's "never rises" are restated (the key stays fixed by the one-state, anchor-block rule, so a later deposit
    moves only panels anchored after it). T17 and D-10 test the re-registration route until then.
* **Eq on t12** takes `min(C, 3 · G_eq)` (§3.6 Eq), not the whole collateral. A genesis bond (939,063.21 MSK)
  keeps at least 748,265.74 MSK after one Eq of the 2M class (190,797.47) and 929,460.32 after one floor Eq, so it
  stays above every floor and keeps seating. Two genesis Eq slashes leave the population at eight (T76). The
  mainnet value is a separate decision.

**B-5.** Owner A (S). Test T05. Vesting is never collateral (V-6).

### 3.2 SR — the staged reservation

**SR-1 (the one expression; X2, F8, C1, C2).** Owner A (S). Tests T01, T02c, T27, T68.

`palw_claim_commitment_v1(params, claim, now_daa) -> Option<u128>` replaces `palw_claim_bond_reservation_v1`
(STATE:2765 at `e93be0f2`) for reserve and release, the load-time re-derivation, every ceiling and the producer
headroom. R-core's per-claim fields (`licence_door`, `basis_k`, `escrow_released`, `served_mask`, `unserved_seen`)
live in `PalwClaimStateV2` after the audit's `job_identity`, **nested** as `claim.rcore: PalwClaimRcoreV1` (§6 rows
2–6, post-edit 9, S-SPEC §1b; byte-identical to appending them flat; v3's separate `claim_rcore` side record is dropped
in v3.1, because F1 already changes the claim record). **The field paths are `claim.rcore.<field>`**, as written in
the rules below. `now_daa` is the DAA of
the point the caller reads at: the block being folded, or, at load, the state's last point (as
`palw_claim_is_on_abandon_hold_v2` is called today).

Let `esc = claim_escrow_reservation_v1(...)` and `rr = rights_reserved`.

| phase | commitment |
|---|---|
| `Provisional`, `PanelBound` (DA sessions open or not) | `w + esc + rr` |
| `ReceiptLicensed` with `claim.rcore.escrow_released = true` | `w + rr` |
| `ReceiptLicensed` with `claim.rcore.escrow_released = false` | `w + esc + rr` (to Final) |
| **`Voided { reason: BindTimeout }`, free-prompt source, `palw_claim_is_on_abandon_hold_v2(claim, params, now_daa)`** | **`w + esc + rr`** (`esc` is 0 on the FP lane): audit C5's abandon hold, `fp_abandon_hold_daa` = 600 on t12 (`palw_fp_devnet_v3.rs:197`), kept unchanged |
| `Final`, every other `Voided`, retired | 0 |

The abandon-hold row is today's rule (STATE `void_claim` keeps the reservation and re-arms `voided_daa + hold`; the
sweep's terminal arm calls `release_abandon_hold`). v3's draft dropped it, which would have re-opened a free,
repeatable free-prompt timeout and panel re-roll (C2), contrary to D1. The hold is a delay, never a forfeit; it is
not charged, and its release arm is DL-1's terminal deadline for that claim.

v2's `DefaultDisputed` rows are gone from this table: past `palw_rcore_plus` a DA session never changes a claim's
phase (DA-2).

`escrow_released` is set iff **all** of the following hold:

1. **The door is Quorum (V1) or Coverage (V2)** and the set's recounted `basis_k ≥ 2` (Q-3). Optimistic (S2) holds.
   (ShardPart is dormant on t12, Q-3.)
2. **Every seat of the panel has served.** "Served" is a **carried `Valid` receipt** (V2 or V3), and nothing else
   (D3: "only a Valid, or a receipt carrying a signed delivery digest, counts as served").
   * `Unavailable` (Withheld), `Incapable`, **`Sampled`**, or a missing receipt **holds** to Final (F8, C1).
     An honest node decides `Incapable` before it looks at deliveries (PANEL:4614-4622 at `4064364e`), so
     `Incapable` proves nothing about service. A `Sampled` receipt proves only that the seat received its S3 sites,
     which are a public function of the bind (`sample_draw(anchor, claim_id, seat_index, counter)`,
     `palw_layer_sample_v3.rs:27-37`): a producer can serve an honest seat exactly its 8 predictable sites and
     withhold the rest, so counting `Sampled` would re-open F8 with `Sampled` in place of `Incapable` (C1).
   * A receipt with a signed delivery digest is not introduced (§9.2, deferred). Holding is the safe direction.
   * The chain reads only carried receipts; a block producer cannot earn the release by omitting a Withheld.
3. **The claim was never redrawn** (`rebound_daa` is `None`).
4. **The class is not in C7 (U1, post-edit 12).** A C7 claim (the 2M row, SR-5) holds `w + esc + rr` to Final
   whatever its licence; the 8k row releases by conditions 1–3 like the floor. This matches the panel-room restoration
   keyed on C7 (T-2(b), post-edit 5). S-SPEC §3.2's `palw_rcore_release_due_v1` carries it as its last condition,
   written `class_id ∉ params.rcore_conservative_classes` (the C7 mirror, §6); since C7 is the list **united with** the
   window rule (SR-5), the test is C7 membership by either, the same predicate T-2(b) reads.

`palw_claim_commitment_v1` takes **three** arguments, `(params, claim, now_daa)` (post-edit 10(a)); `phase2-plan.md`
§2.3's two-argument form is superseded.

**SR-1b (a completing receipt flips the release; F11, IMPL-1).** Owner A (S) in the licence and the V2 door; B (M4)
through SR-10's V3 door (post-edit 13). Test T74.
* The fold keeps `claim.rcore.served_mask` (one bit per panel seat, set by a carried `Valid` only) and
  `claim.rcore.unserved_seen`, updated by the licensing object **and** by supplementary receipts: the V2 door
  (`credit_supplementary_receipts`, ADR-0124 D2) and SR-10's V3 door (from M4; until then only the V2 door, which
  takes V2 receipts, so a partial seat's V3 receipt cannot complete a release).
* If a supplementary receipt lands at `daa ≤ min(licensed_daa + ⌊window_challenge_at/2⌋, bound_daa + window_receipt)`
  (= `min(L + 60, bound + 600)` on t12; the second term because both doors refuse receipts past the receipt
  deadline, STATE `credit_supplementary_receipts`) and makes conditions 1–3 true, `escrow_released` flips to `true`
  and `esc` is released in that block. A flip never happens later, and never un-flips (SR-4).
* A `Sampled` supplementary receipt is credited for pay (§9.1 Q3) but never sets a `served_mask` bit.
* **One rule for both doors (IA-5; `6b5af6f5`).** The flip is `palw_rcore_supplementary_flips_v1`: the claim is
  `ReceiptLicensed`, the flag is unset, the set lands by `palw_rcore_release_window_closes_v1`, and
  `palw_rcore_release_due_v1` holds on the record as the set staged it. S-2's V2 door (`stage_supplementary_v1`) and
  F4's V3 door (`credit_supplementary_receipts_v3`) both call it. The flip moves the producer's ledger in the same
  write as the record (`move_commitment`, SR-3), so it can never flip twice. A `Sampled` set latches `unserved_seen`
  and so never flips. The empty seam F4 left (`rcore_supplementary_release_seam_v1`) is gone.
* So p no longer depends on who carried the licence first. p is still griefable: a silent seat holds the escrow
  for free. That is stated, not closed (§3.7 T-2(d)).

**SR-2 (recorded).** Owner A (S). Tests T01, T28. At licence the claim record records `licence_door`,
`basis_k` and `escrow_released`. All three are copied into the vesting row and the liability record (X3).
* **The door does not determine `basis_k` (IA-5).** `staged_licence_v1` records the carried door and Q-3's recount
  over the distinct `Valid` signers' masks. When a supplementary set first raises an S2 claim to `basis_k ≥ 2`, the
  door is re-recorded **Coverage** if a counted mask is partial, otherwise **Quorum**, with the recounted `basis_k ≥ 2`
  (2 when one seat is added; the V2 door takes `min(3, old_k + added)`, so one set adding two new `Valid`s records 3;
  `stage_supplementary_v1`, `credit_supplementary_receipts_v3`). So a `Quorum` record can carry `basis_k` 2, and the
  door's name never sets `k`. L-1's price and V-8's tick read
  `basis_k`, never the door's name. SR-1 cond. 1 reads both, a Quorum or Coverage record and `basis_k ≥ 2`
  (`palw_rcore_release_record_holds_v1`), and an upgraded S2 claim meets it through its re-recorded door.

**SR-3 (one funnel, fence-aware; X16).** Owner A (S). Tests T01, T40. Every claim phase write, and every write of the
claim's R-core fields, adjusts `reserved_exposure(bond)` by `commitment(before) − commitment(after)` and re-notes the
claim in the in-flight index (T-2(c), IMPL-9). The abandon hold's release is the one time-driven change: its
terminal deadline (DL-1) writes the release in the sweep, as today. The fence reaches the commitment through the
`#[borsh(skip)]` mirror `PalwStateParamsV2::rcore_plus_from_daa` (§6, IMPL-6). `palw_claim_commitment_v1` is a pure
function of `(params, claim, now_daa)`, and `assert_internal_consistency_v3` re-derives it on load from
rooted inputs and the last point only. **As integrated (IA-5):** for a write that keeps the claim live (the licence's
release, SR-1b's late flip through either door) the funnel is `move_commitment`, checked both ways, so a double release
fails deterministically on every node.

**SR-4 (monotone).** Owner A (S). Test T14. A licensed claim never re-reserves. `escrow_released` never goes back to
`false`.

**SR-5 (what a failed claim forfeits; D1, D7).** Owner A (S), then M6. Tests T02, T02b, T20.

| void | before `palw_rcore_attributed_charging` (launch) | after it (X10) | C7 classes (2M; D7) |
|---|---|---|---|
| `BindTimeout`, `NoCapablePanel` | S0: no forfeit (the FP abandon hold, SR-1, is a delay, not a charge) | S0 | S0 |
| first failed panel: first `ReceiptTimeout`, SR-9 early redraw, **Q-5 gate on the first panel** (all redraw) | no charge | no charge | no charge |
| **second failed panel**: second `ReceiptTimeout`, SR-9 `UnavailableQuorum`, Q-5 `NotReplayBacked` | **S0′: forfeit the commitment at the stage** (`w + esc + rr`), no strike, no action tier, no reward | S0 | **S0′ always** (until ADR-0153) |
| DA-confirmed withholding | S1 | S1 | S1 |
| `CourtFraud`, `ExecutorRefuted` before Final | S2 | S2 | S2 |

* **Decision 7's "no S0" (C13)** covers the producer-attributable failure voids of the second panel (RT#2,
  `UnavailableQuorum`, `NotReplayBacked`), which stay S0′ on C7 after X10. Capacity voids (`BindTimeout`,
  `NoCapablePanel`) and every first-panel redraw stay uncharged on every class, as they are today
  (`void_and_slash` leaves `BindTimeout | NoCapablePanel` uncharged; STATE:13667 at `e93be0f2`).
* **Only the second failed panel forfeits (V3S-01).** The v3 draft charged a first-panel `NotReplayBacked` S0′. One
  silent or `Sampled` partial seat, or an honest partial seat that was late, could then sink an honest S2-licensed
  claim with no redraw: with m Sybil operators holding a partial position on an honest floor panel with probability
  0.444 / 0.667 / 0.788 at m = 1 / 2 / 3, the expected forfeit was 1,422.64 / 2,133.97 / 2,521.96 MSK per honest floor
  claim (8k 1,642.30 at m = 1), at no cost to the griefer (`v3_numbers_r2.py`, reproducing the verifier). D1 kept the
  forfeit only for two failed panels, so the first-panel gate redraws (Q-5).
* **S0′ is today's #10**, kept (D1). The review measured its griefing cost as 800 MSK per floor claim at 8
  Sybils (not recomputed here) and the rational fake-root EV it prevents as +485 MSK/attempt at 4 Sybils. An
  operator-selected cap `min(commitment, c_RT2)` is allowed by D1; **no cap** (`c_RT2 = ∞`; decided, §9.1 Q1).
* **`palw_rcore_attributed_charging`** is absent at the regenesis. **S does not declare it (post-edit 9, S-SPEC §1e):**
  the field and `PalwTransitionExtrasV1::rcore_attributed_charging_active` are declared and armed together with M6.
  When M6 adds it, it is a separate `Option<ForkActivation>`, hashed Some-only with the `never()` collapse (so adding
  it moves no network's fingerprint), with a `fork_id_v1` entry, and `validate_palw_v2` refuses it unless
  `palw_rcore_plus` is armed at or below it. It is armed on public t12 by a flag day only after M1–M5 are GREEN
  **and** O-3 (§8.4) shows the garbage and borrowed strategies convicted live. The arming build needs a drill that
  crosses its height (work rule "a flag day needs a drill that crosses it"). At the regenesis RT#2 is S0′ on every
  class, and T02b is restated accordingly (§8.1).
* **C7** (the conservative set, D7) is `Params::palw_rcore_conservative_classes`, set by name in
  `palw_t12_arm_every_rule_from_genesis` to the 2M class id, plus any class registered later whose verification
  window is ≥ `PALW_RCORE_C7_WINDOW_SPANS_V1` = 1,000 spans (decided, §9.1 Q2). T20 asserts the
  genesis set is exactly the 2M row (window 2,799 spans; the 8k row's window is 3).
  * **C7 is the one set the conservative rules key on (post-edit 5, correcting V3S-09).** C7 =
    `verification_window_spans ≥ PALW_RCORE_C7_WINDOW_SPANS_V1` (1,000), **united with**
    `palw_rcore_conservative_classes` once R-core+ lands; on the launch line before S (the room fix on
    `fix/t12-panel-room`) the window rule alone selects it, and at t12 genesis both select exactly the 2M row. C7
    carries: the panel-room hold to Final and to its static cap (T-2(b)); the escrow held to Final (SR-1 cond. 4, U1);
    the charging rule (S0′ after X10); and L-4b's top-up. The held-context rows (`PALW_T12_GENESIS_HELD_ROWS`, the 8k
    and the 2M row, `class_is_held_v1`) keep only what a held context means for the artifact and DA (held units,
    DA-3); **the 8k row is released at licence** like any class outside C7.
  * It is hashed into the fingerprint Some-only, when non-empty, beside `palw_rcore_plus` (IMPL-6), and
    `validate_palw_v2` refuses a non-empty list without `palw_rcore_plus`, or a list that is not a subset of
    `PALW_T12_GENESIS_HELD_ROWS` (S-SPEC §5).
* A **post-licence** S1 or S2 on a claim whose escrow was released also takes `E` from uncommitted collateral
  (X7), so non-disclosure is never cheaper than conviction.

**SR-6 (license on the backed subset; X9).** Owner A (S), B (assemblers, P2-5). Test T33. Unchanged from v2, with
the lock price of Q-4. Past this fence a
receipt set whose carried `Valid`s include an unbacked signer licenses on the backed subset if that subset still
meets the door's rule (quorum, coverage, outsider). Unbacked `Valid`s get no lock, no credit and no liability. The
acceptance predicate (`palw_v2_object_licenses_claim_v1`, STATE:14329) uses the same subset. The V1 and coverage
assemblers skip unbacked candidates (Phase 2, P2-5).

**SR-7 (admission on the live state).** Owner A (S). Tests T08, T09. Every ceiling checks
`palw_bond_committed_v1 + palw_claim_commitment_v1(new) ≤ collateral × 500‰`: `apply_attempt`,
`palw_admission_v2.rs:572-648`, `palw_producer_v2.rs:279`, the FP ceilings and the draw (`palw_panel_v2.rs:702`).
Committed amounts now depend on lock liveness at the current DAA and escaped depth, so the draw
(`derive_panel_v2*`) takes `now_daa` and the escaped depth, applied identically by the template, the acceptance layer
and the fold (IMPL-14).

**SR-8 (ExecutorRefuted).** Specified in §3.10 J-4 (F1/F2). v2's text is superseded.

**SR-9 (early redraw on ≥ 3 Unavailable; F12).** Owner A (S), B (the node filer in P2-6). Test T57.
* **Carrier.** A new object `PanelUnavailableQuorum { claim: Hash64, receipts: Vec<PalwSeatReceiptV3> }` (tag 56)
  carries `seat_count − quorum + 1` (= 3 of 5) signed `Unavailable` receipts of the claim's **current** panel, each
  inside its receipt window. The retired `ProducerDefaulted` stays refused (`ProducerDefaultRetired` under
  `palw_unavailable_abstains`): this object slashes nobody, and `Unavailable` still abstains.
* When it lands on a claim's **first** panel, the claim takes its one redraw in that block, exactly as the first
  `ReceiptTimeout` would, instead of waiting out `window_receipt` (600).
* On the **second** panel, the same object voids the claim `UnavailableQuorum` (a new `PalwVoidReasonV2`,
  appended). It is charged like RT#2 (SR-5). After X10 it is S0, and any seat may still convert it into S1 through
  DA.
* A panel with ≥ 3 `Unavailable` cannot reach a quorum of 3, so nothing licensable is lost.
* A redraw never closes an open DA session (DA-5): a withholding producer that is redrawn still answers or defaults.

**SR-10 (the V3 supplementary door; IMPL-1).** **Owner B (M4)** for the door, its `served_mask` bits, SR-1b through
it, the recount, the upgrade and the mask locks of Q-5 (post-edit 13; v3.1 had the door in S and S-SPEC §3.5/S-5
still plans it there). A (S) keeps `served_mask` and SR-1b at the licence and in the V2 door, and the `redraw_claim`
helper. Tests T74 (the V3 half), T72.
* **Why.** Today's supplementary door (`credit_supplementary_receipts`) takes only `PalwSeatReceiptV2` and refuses
  every verdict but `Valid`; t12's partial seats sign V3 receipts over `palw_receipt_message_v3`, which cannot be
  re-presented as V2. So neither SR-1b nor Q-5's upgrade could be carried for a partial seat, and an honest S2
  licence could never reach `basis_k ≥ 2`.
* **Why it matters at launch (the licence-stall follow-up, post-edit 13).** After the licence-stall fix
  (`fix/t12-licence-stall`) an optimistic licence credits at most 2 seats, the full seat and at most one partial seat,
  unless all five validated: then all five ride, never three or four, and the coverage door normally takes that set
  first (`palw_select_optimistic_licence_v2`'s doc, `palw_panel_v2.rs:1886` at `c8652a97`; licence-stall report Fix 1).
  A two-seat licence recounts to `basis_k = 1`, and t12 has no other supplementary path for V3 receipts, so an S2
  claim upgrades only through this door, which lands in M4. Before M4 an S2 claim finalizes at
  `L + window_challenge_at` with its escrow held and no anchor tick (S-SPEC §3.9); nothing launches in that state,
  because M4 is in the gate (§8.3).
* **The door.** Past `palw_rcore_plus`, the existing `ReceiptLicensedV2` object (no new tag) is also accepted on a
  claim that is already `ReceiptLicensed`, as a **supplementary set**: V3 receipts (`Valid`, `Unavailable`,
  `Incapable`, `Sampled`) of panel seats not yet credited, each verified under `palw_receipt_message_v3`, each
  signed in `[bound_daa, bound_daa + window_receipt]` and at or before the carrying block, the object landing at or
  before `bound_daa + window_receipt`.
* **Effect**, in one funnel: credit the new seats (`Sampled` for pay only, §9.1 Q3); set `served_mask` bits for new
  `Valid`s and `unserved_seen` for `Unavailable`/`Incapable`; lock each newly counted `Valid` signer at
  `lock_{max(basis_k′, 2)}` with its mask (L-3); recount `basis_k` over the licensing and every supplementary
  receipt (Q-3); record the door upgrade (Q-5); apply SR-1b; settle the anchor the first time `basis_k` reaches 2
  (V-8).
* **Masks reach the lock.** `lock_valid_receipts` takes `(receipt, mask)` pairs. The V2-coverage and S2 licence arms
  stop stripping masks (`r.receipt.clone()` before `lock_valid_receipts`, STATE:18746 / :18784 at `e93be0f2`); a V2
  receipt is passed with the full mask. The V2 door keeps working past the fence and also recounts.

### 3.3 V — vesting rows

**Ownership (post-edit 9).** The vesting rows, step 3d (latch → plan → move, `phase2-plan.md` option A), `VestingNote`
and A-KEY are written by **B** inside S's window, not by the audit's S (S-SPEC §1h, §9 P5, P11, P12; the handoff's
split). S calls B's two hooks, `burn_vesting_row(claim_id, offence_id, kind) -> Result<Option<u64>, _>` (`Some` is S3's
once-per-claim marker; S ships a stub) and `vesting_row(claim_id)` (the X29 prune guard), and writes `reward_pending`,
`reporter_rewards` and the reporter counters at step 2 only; B's 3d moves `reporter_rewards` first and deletes them.
Where a V-rule below says "Owner A (S)" for the row machinery, read **B (S window)**; the tests are unchanged.

**As integrated (IA-6, IA-7).** The vesting work (`d3ece0d5`) replaced S's stub with `burn_vesting_row`'s body, and
the integration line sets **`PALW_RCORE_VESTING_ROWS_LANDED_V1 = true`**. The flag says that a Final's escrow `E` stays
recoverable after `Final`, which is what lets L-1 price the lock on the residual `G − E` and SR-1 release `E` at
licence. It may be `true` only while three things hold. `finalize_claim` writes the row instead of paying `E` (V-2,
`write_vesting_row_at_final`). `burn_vesting_row` burns it. And **every** post-Final conviction reaches that burn
through S-4's funnel (`post_final_producer_leg_v1`): S3 for kinds 3 and 4, for a proven court verdict and for the
FinalRow DA default, and U3 for a free-prompt claim, which writes no row. Before the funnel only the DA default burned a
row, so the flag stayed `false`: a residual price with no burn behind it under-collateralizes the gain. While the flag
is `false`, L-1 prices every lock on the whole gain (`palw_rcore_lock_unvested_v1`), and **kaspad refuses to start**
a network that arms `palw_rcore_plus` (`palw_rcore_build_can_run_v1`, `ConfigError::PalwRcoreVestingRowsNotLanded`,
the S re-review's N1). Tests: `rcore_whole_gain_and_buyback` asserts the flag; `dos_l5_4b` licenses every claim
without the stall that the whole-gain price caused; `dos_l5_6` keeps `daa_2m_dead == 0`;
`n1_rcore_plus_needs_the_vesting_rows_at_startup`.

**V-1 (layout; X3, X18, X29).** Owner A (S). Tests T03, T11, T41. A rooted map `vesting: BTreeMap<Hash64 /*claim_id*/, PalwVestingRowV1>`,
separate from `claims`, outliving claim retirement (`claim_retirement_daa` = 3,000). **v3.1:** the row copies the four
attribution fields the audit appends to `PalwPanelLiabilityRecordV1` (`job_identity`, `free_prompt`, `trace_root`,
`segment_count`), as agreed with the audit session; it never resolves them through the liability row, so the row alone
can bind a conviction (J-2). `finalize_claim` copies them from the claim record and its panel in the same funnel as
`persist_panel_liability`, under the same write rule (non-zero only when `offence_attribution_active`).

```rust
pub struct PalwVestingRowV1 {
    pub claim_id: Hash64,
    pub producer_bond: PalwBondKeyV2,
    pub class_id: Hash64,
    pub execution_root: Hash64,
    pub artifact_root: Hash64,                     // R-core's own: the execution check's artifact after retirement
    pub job_identity: Hash64,                      // v3.1: COPIED from the liability record (SPEC §4.1; agreed)
    pub free_prompt: bool,                         // v3.1: copied (the lane, for the identity checks J1/J5)
    pub trace_root: Hash64,                        // v3.1: copied (identity check J4)
    pub segment_count: u16,                        // v3.1: copied (a V3 receipt's liability after retirement; 0 = unknown)
    pub licence_door: PalwLicenceDoorTagV1,        // X3
    pub basis_k: u8,                               // v3, F4: the recounted k of the Final-basis set (2 or 3)
    pub escrowed_reward: u64,
    pub buyback_bound: u64,                        // s
    pub producer: PalwPayoutV2,                    // payload fixed at Final
    pub seats: Vec<(PalwBondKeyV2, PalwPayoutV2)>, // credited seats, per claim
    pub reserve: u64,
    pub final_daa: u64,
    pub expiry_daa: u64,                           // palw_panel_liability_expiry_v1(final_daa, window_court); DA-5 may extend it
    pub settled_at_final: u64,
    pub matured_at: Option<u64>,                   // latch (X29)
}
```

Also rooted: `reporter_rewards` (§3.6 R), and the counters `vesting_created_sompi`, `vesting_moved_sompi`,
`vesting_burned_sompi`, `reporter_awarded_sompi`, `reporter_forgone_sompi`.

**V-2 (what vests).** Owner A (S). Tests T03, T13. Unchanged from v2: `finalize_claim` (STATE:13641) writes the row where it writes payouts
today (`split.producer`, each credited seat's `per_seat`, `split.reserve`, or the whole reward with no duty row).
Not vested: the ADR-0091 buyback slice (priced into `G_res` as `s`), the work-price remainder, FP receipt-spend
payouts, execution-lane rights.

**V-2b (execution-lane rights of a convicted Final; Phase 2 plan §5.3).** Owner A (S). Test T81.
* Every conviction of a claim writes a consumed offence and forfeits the claim's **unminted** round rights, by one of two
  routes (SPEC §4.5, v3.1):
  * **by root** — the contradiction proves the committed execution itself bad: contradictions `StepArithmetic` (5),
    `StepStructural` (6), `ForgedOutput` (8), `ForgedOutputTiled` (11), `LogitsNotStepOutput` (12), in `PanelFalseValidV2`
    or `ExecutorRefuted`. (The numbers are `PalwPanelContradictionV1` discriminants; "kind" is kept for
    `PalwOffenceKindV1`, R7.) The record carries `execution_root` (only when economic safety is armed, SPEC §3.5 step 6) and
    ADR-0151's `palw_forfeited_execution_roots_v1` (STATE:7468 at `e93be0f2`) drops that execution's rights;
  * **by claim** — the conviction proves the claim wrong but not its root: contradictions `IdentityMismatch` (9),
    `OutputMismatch` (10) and `PromptNotAnchored` (13, post-edit 1), `ProducerWithholding`, `CourtFraud`, and the
    offence kinds `DaDefault` (5) and `CourtConviction` (6). The
    record carries `execution_root = 0` and the writer calls SPEC §4.5's `forfeit_minted_round_rights_of_claim(claim_id)`.
    A borrowed root is some honest execution's; forfeiting by root would take the lender's rights. v3 recorded the root
    for DA defaults and court convictions; that is withdrawn.
  * **The route is the finding's `forfeit` (post-edit 2).** F2's `PalwFalseValidFindingV1` no longer carries
    `execution_proving: bool`; it carries `acts_on_claim: bool` and `forfeit: PalwForfeitScopeV1 { None, ByClaim,
    ByRoot }` (ADDENDUM §4-bis.7). In `consume_false_valid_v2` and in kind 4, `ByRoot` records the root and runs
    `forfeit_minted_round_rights` (the route for 5, 6, 8, 11, 12); `ByClaim` records 0 and runs
    `forfeit_minted_round_rights_of_claim` (9, 10, 13). R-core's own writers (DA defaults, court convictions) take
    `ByClaim`.
* **Every conviction path writes one (IMPL-3).** Today only `consume_objective_offence` writes `consumed_offences`
  and calls `forfeit_minted_round_rights` and `reverse_convicted_final`. Past `palw_rcore_plus`:
  * `PalwOffenceKindV1` gains `DaDefault = 5` and `CourtConviction = 6`, appended after the audit's
    `PanelFalseValidV2 = 3` and `ExecutorRefuted = 4` (v3.1);
  * **only the fold writes them; a filed one is refused by name (R1).** `PalwOffenceKindV1` is the `kind` a filed
    `ObjectiveOffence` (tag 51, STATE:4274-4279) carries on the wire, so past `palw_rcore_plus` every exhaustive
    `match kind` gets an arm for 5 and 6, following `CourtExecutorGuilty`'s precedent: `palw_ledger_evidence_id_v1`
    (OFF:174) and `palw_verify_objective_offence_v1` (OFF:381, beside `CourtGuiltyIsNotStandalone` at OFF:388) return
    the new `PalwOffenceVerifyError::DaDefaultIsNotStandalone` / `CourtConvictionIsNotStandalone`, and the fold's
    `consume_objective_offence` refuses both beside `CourtExecutorGuilty` (STATE:11301-11306) with
    `ObjectiveOffenceRefused(evidence_id, "DaDefault is written by the DA sweep, not filed")` and its
    `CourtConviction` twin. Neither arm is ever `unreachable!`. The arms refuse whatever the fence says (the variants
    exist in the enum on every network once v22 appends them), so no filed object writes either row anywhere; on a
    network without the fence the object is dropped and the block stands, as an undecodable kind was before (SPEC §0,
    premise 3);
  * DA-7's default writes a `DaDefault` record under
    `palw_offence_id_v1(DaDefault, producer_bond, H(PALW_DA_OFFENCE_KEY_DOMAIN_V1 ‖ claim_id))`; the court's `CourtFraud`
    void writes a `CourtConviction` record under
    `palw_offence_id_v1(CourtConviction, producer_bond, H(PALW_COURT_OFFENCE_KEY_DOMAIN_V1 ‖ claim_id))`. Both are one
    per claim. The court key is per claim, not per session (v3 said `session_id`), because the shard and checkpoint
    courts void `CourtFraud` in one move with no session (STATE:17376, :17436 at `0533e1de`; the audit's S spec, P10).
    A held `StepLeaf` default is a DA default (DA-1 moves the held demand into the session);
  * each record carries `execution_root` (0 on the by-claim route), `claim_id` and `collected` (R-2), and its writer calls
    the forfeiture of its route and, for a Final claim, `reverse_convicted_final` and the audit's
    `mark_liability_convicted`, exactly as the audit's `consume_false_valid_v2` does;
  * Phase 2's `PalwVestingNoteV1::Burned { kind, offence_id }` can then name every conviction.
* **Residual, named:** on t12 execution quanta mature at `F + window_challenge` = F + 1,200
  (`palw_exec_quantum_maturity_daa_v1`, `palw_economic_safety_v1.rs:106`), before the row's F + 3,000. Rights
  that matured before the conviction are not revoked, and the round fees they earned are market-driven, not
  bounded by `R`. §9.1 Q5 keeps that maturity unchanged.

**V-3 (deltas, carriage, root, consistency; X16, X18; Phase 2 plan §5.3).** Owner A (S), B (T03). Tests T03, T40, T41,
T48.
* **Deltas.** `Vesting`, `ReporterReward`, `RewardPending`, `ReporterCommit`, `DaSession`, `DaClaim`,
  `Strikes`, one entry per counter, and the no-op journal `VestingNote` (Phase 2 plan §2.5), all
  appended after `AnchorDaaPruned`.
* **Root.** Hashed Some-only when non-empty. **Carriage.** Appended (v22).
* **Derived indexes.** The rows by `(expiry_daa, claim_id)`, the rows by payee bond, the DA sessions by deadline,
  the reveal windows by deadline, and (IMPL-14) duties by seat bond, reporter commitments by reporter and by
  `committed_daa`, and unlicensed claims by `(bond, class)` are derived, delta-maintained on apply and revert, and
  rebuilt on import (§6 row 29).
* **Consistency.** Every row amount is ≤ `escrowed_reward − buyback_bound`; a row exists only for a claim that is
  `Final` or retired; `vesting_created = Σ live rows + vesting_moved + vesting_burned`.
* **The coinbase identity (test-only, T03), restated.** Over a simulated chain:

  ```
  Σ withheld (own + merged escrow carve)
    = Σ minted-from-rows + Σ live vesting + Σ burned (voids, row burns, skipped carve)
    + Σ unnamed (work-price remainder) + Σ buyback executed at Final + Δ panel_reserve_sompi
  ```

  Reporter and market mints are excluded: they are funded by slashes and sinks, not by escrow. Minted-from-rows
  is attributed by position in the coinbase and by the parent queue's keys. The fold emits `VestingNote`
  entries for the buyback slice and the reserve credit so the identity closes from deltas alone: the vesting work
  appends them to `PalwVestingNoteV1` as tag 5 `BuybackAtFinal { claim_id, sompi }` and tag 6 `ReserveCredited
  { claim_id, sompi }`, each emitted only when positive (IA-6).

**V-4 (maturity, the fold order, and Mainnet Decision A).** Owner A (S). Tests T04, T12, T37, T44. A row is mature
iff (a), (b) and (c) hold.

* **(a) The lock predicate.** The negation of `PalwSlashableLockV1::is_live_v3` over the row's
  `(expiry_daa, settled_at_final)`, at the escaped depth. The same two clocks as the lock (v1 decision 4).
* **(b) The chain is not in a licence halt:** `palw_chain_vesting_halted_v1(state, raw_depth, now, window_court)`
  is false, where halted = `raw_depth.is_some() && palw_second_clock_depth_v1(...).is_none()` (Phase 2 plan F11:
  "no second clock configured" is not "halted").
* **(c) No DA session is open on the row's claim** (DA-5). Past this revision (c) is implied by (a): a session
  re-keys the row to at least its deadline + `window_challenge_at` when it opens (DA-5, V3S-02), and the fold asserts
  the implication rather than relying on it.

Once (a)–(c) hold at a maturity step, `matured_at` is latched. A later re-arm cannot un-mature a row.

> **Mainnet Decision A does not apply to vesting rows, and must not be applied to them later.** A vesting row is
> PALW state, not a coinbase output. Once the payout exists, its output obeys Decision A unchanged. DAA-only
> vesting would let heartbeat-only history release a fraud's reward; a two-clock coinbase would re-create the
> liquidity dependency Decision A removed. (v2 text, kept.)

**The fold's order (X4), at `e93be0f2`, with v3's insertions in bold:**

| step | what | where |
|---|---|---|
| 1b | drain the first 8 queue rows | `apply_palw_transition_v7`, STATE:14388 |
| 1c′ / 1d | EVM settlement list; execution-lane span boundary | |
| 2 | `sweep_deadlines` (Finals → rows; RT#2; the Q-5 gate: first-panel redraw or `NotReplayBacked`; the FP abandon-hold release; retirement, which burns unrefunded `refuted_held`), **`sweep_da_sessions` (DA defaults, DA-7)**, **`sweep_reward_reveals` (R-4: closed windows → `reporter_rewards`)**, `sweep_court_close_deadlines`, `sweep_court_deadlines`, `sweep_fp_prompt_rows`, `sweep_panel_obligations` (pruning, row-guarded), **`sweep_reporter_commitments` (TTL, guarded by open pending rewards)** | STATE:16807, :16760 (at `e93be0f2`) |
| 2b–2d | retarget, share raise, reclamation | |
| 3 | objects in acceptance order: licences, supplementary receipts (the V2 door and **SR-10's V3 door**: SR-1b, Q-5 upgrade), convictions, `ExecutorRefuted`, DA accusations (re-keying a row, DA-5) and **`MaterialDisclosedV2`**, **`ReporterCommitted` / `ReporterRevealed`** | |
| 3′ | carrier refunds | `apply_carrier_market_refunds`, STATE:20763 |
| 3c | EVM market | `apply_evm_market_actions`, STATE:20678 |
| **3d** | **maturity: latch, then plan, then move (V-7)** | new |
| 3a, 3b | activation, budgets | |
| 4, 4b | own work, mergeset | |

Maturity is post-object: it reads the clocks after this block's licences, and a same-block conviction or DA
default burns first. The pre-object mirror (`palw_v2_pre_object_base_v1`) must mirror step 2 exactly, including
the two new sweeps; the processor's queue rehearsal is untouched because nothing before 3d writes the queue
(Phase 2 plan F4, invariant I-3).

**Honest waits** (v2's table, unchanged; a DA session on the claim adds its length):

| path | DAA from A | 120 s/DAA | **200 s/DAA (measured)** |
|---|---|---|---|
| nominal, to mint | 3,147 | 4.4 d | **7.3 d** |
| nominal, spendable (+600, Decision A) | 3,747 | 5.2 d | **8.7 d** |
| floor redraw, spendable | 4,943 | 6.9 d | 11.4 d |
| trickle (< 30 licences per 3,000), spendable | 9,748 | 13.5 d | 22.6 d |
| 2M, spendable | 9,341 | 13.0 d | 21.6 d |
| 2M on a quiet chain | 15,341 | 21.3 d | 35.5 d |
| licence halt | + halt length | | |

**V-5 (burn on conviction, keyed on the row; X1, X3, X6, X29, F21).** Owner A (S). Tests T04, T26, T28, T46h, T46k.
* **Binding.** A conviction names a `claim_id`. The audit's adjudicator resolves its target with
  `palw_offence_target_v1` (the claim record, else the liability record; SPEC §3.3); past `palw_rcore_plus` R-core adds
  the vesting row as the third source (J-2), which carries the copied attribution fields (V-1). The burn runs whenever a
  row exists. The #8 reversal of weight and probe runs only while the claim record exists.
* **X1 / F21 (v3.1: refused by name).** Past `palw_offence_attribution`, `CourtExecutorGuilty` is refused as a
  contradiction (`ContradictionNotAdmitted`; no code writes that consumed row), and kind 1 is refused entirely
  (`SupersededOnThisNetwork`). The v3 zero-root rule is kept for the kind-1 path below the fence, where T26 still
  asserts it. The zero default must never "agree with everything".
* **Convictions that delete the whole row:** every accepted `PanelFalseValidV2` and `ExecutorRefuted` conviction of the
  claim (contradictions 2, 4, 5, 6, 8, 9, 10, 11, 12, 13 as SPEC §3.3 step 7, §4.4 and ADDENDUM §4-bis.7 admit them), a post-Final DA default
  (DA-7), and a post-Final court conviction. The row's deletion is S3's once-per-claim marker (§3.6). **As integrated
  (IA-7, IA-9):** each of these reaches the burn through S-4's `post_final_producer_leg_v1`, which calls
  `burn_vesting_row(claim_id, offence_id, kind)` and charges S3 only when the hook reports the row it burned.
* **S4′ is unreachable on t12 (v3.1).** `ConflictingPermit` is refused by name past `palw_offence_attribution` (it names no
  claim), so v3's "one share burns, the claim stays Final" arm (X6) is dead code on t12 and is kept only below the fence.
* **Liabilities and locks are not pruned while their claim's row exists (X29).**
* A row is burnable until it is **moved** to the queue, including while latched but carried. Burned value goes to
  `vesting_burned_sompi` and is never minted.

**V-6 (no escape).** Owner A (S), B (UTXO half, P2-1). Test T05. Unchanged: a row is not a UTXO; it is absent from collateral, committed, readiness,
ceilings and withdrawal amounts; the payee is fixed at Final.

**V-7 (mint budget; X5, X30, A-KEY, backlog).** Owner A (S). Tests T16, T30, T47, T82.
* At step 3d, move reporter rows first, then vesting rows in `(expiry_daa, claim_id)` order. Stop at the first
  row that is not latched-mature or does not fit. **Never skip.** A row with an open DA session is never at the
  head of that order while an earlier-keyed row is mature: its session re-keyed it past the session's deadline
  (DA-5, V3S-02), so one session cannot freeze the queue.
* **Keys (A-KEY).** A moved producer leg is written under `palw_vesting_payout_key_v1(claim_id)`
  (`H(DOMAIN_VESTING_PAYOUT ‖ claim_id)` with byte 0 forced to `0x00`); a reporter leg under
  `palw_reporter_payout_key_v1(offence_key)` (same form). Seat legs keep `palw_panel_payout_key_v1(payload)`
  (`0xFE`, accumulated per payee). The two new domains are kept out of `PALW_STATE_V2_ALL_DOMAINS`, like the market
  and refund row-key domains. So "maturity keys sort before market keys (`0xFF`)" is now true for every claim,
  including the 1/256 whose raw id begins `0xFF` (T47).
* **Budget unit (backlog).** The budget counts **new queue keys**, not legs: a move costs the number of distinct
  keys it creates that no earlier move of this block created. Seat legs to the same payee share one key. The
  reserve is not a key. **As shipped (IA-6):** the plan is `palw_vesting_mint_plan_v1(state, budget_new_keys,
  market_waiting)` with the budget `palw_vesting_budget_v1`. A row's queue position is `palw_vesting_mint_position_v1`
  = (moves ahead, keys ahead), counted in keys. What moves next block, asked of a committed state, is
  `palw_vesting_next_block_plan_v1` (§7.3). Step 3d moves a reporter reward in the block whose step-2 sweep wrote it,
  whenever the budget has room.
* **Market reserve.** The budget is `8 − min(2, market rows waiting)`. So while the market has rows queued, it
  gets at least 2 of the 8 drain slots, and a post-halt backlog no longer refuses every market move. A 6-key row
  still fits (6 ≤ 6).
* **Queue lemma.** The non-market part of the queue is ≤ 8 after every fold, so every moved leg is minted one
  block after its move (Phase 2 plan F2, T58; the plan's T46, renumbered in v3.1). Maturity inserts stay exempt from `PALW_V2_MAX_PENDING_PAYOUTS`
  (M-10); the queue stays ≤ 1,024 + 8.
* **Throughput after a halt (IMPL-10; model, `v3_numbers_r2.py`).** Each t12 genesis seat is paid at its own key
  (asserted by the test at `params.rs:20751` at `f8c91f19`), and the producer leg is keyed per claim, so a floor
  row with 5 credited seats creates 6 new keys. A second row fits a budget of 8 only if its seats overlap the first
  row's in at least 4 of 5, probability 16/56 = 0.2857 for uniform 5-of-8 panels. So the drain is **1.286 rows per
  block on average with no market row waiting** (P(≥ 2) = 0.285) and **exactly 1 while market rows wait** (budget 6).
  The rule guarantees ≥ 1 row per block (a 6-key row always fits), and the market gets `8 − non-market keys used`
  ≥ 2 slots while it waits. The v3 draft's "2–3 rows per block" was wrong. A licence halt adds no rows (no licences),
  so the backlog after a halt is the rows unmatured at its start, bounded by T-3(b); it drains in at most that many
  blocks.

**V-8 (the escape, closed for rows; X13; F18; C5).** Owner A (S), B (Q-5 upgrade tick, M4). Tests T37, T43, T72; O-6.
* Rows never mature during a licence halt (V-4(b)). If a halt outlasts `F + 9,000`, the row matures at the first
  licence after it, so its conviction must land during the halt. Heartbeat blocks carry conviction, DA and reveal
  objects (H-1).
* **Halt triggers named:** three genesis retirements; `NoCapablePanel`; `InsufficientEligibleBonds`;
  `InsufficientEligibleStake` (SW-10: less than 875‰ of the base draw weight eligible, for example when working honest
  seats are saturated); a censored or heartbeat-only stretch.
* **The trickle cost, named (F18).** A full halt needs about 292 silent Sybil seats (≈ 38M MSK, recyclable) at
  1 claim/DAA. Pushing the chain into the trickle regime (< 30 licences per 3,000 DAA), which delays every honest
  row to F + 9,000 (22.6 d at 200 s), needs about 22 Sybils (≈ 2.9M MSK) at 0.05 claims/DAA. Both are the
  review's figures; the second is UNVERIFIED (it depends on traffic). Under RT#2 (D1) silent seats are not free
  for the claims they sink, but they cost the Sybil nothing.
* **FP licences do not count (F18).** Only attempt licences tick the anchor (`settle_anchor` in `license_claim`
  for `PalwClaimSourceV2::Attempt`, STATE `license_claim`). An attempt-quiet, FP-busy chain therefore counts as halted
  and freezes attempt rows. This is stated, not changed: an FP licence proves a panel for a claim that writes no
  row, and it does not keep attempt rows maturing (§9.1 Q6, decided). O-6 measures how often it happens.
* **S2 licences do not count either (C5, IMPL-17).** Today `license_claim` settles the anchor at every attempt
  licence past the audit fence, the `OptimisticLicensed` arm included, on the premise that "a licence needs a quorum's
  live signatures". That premise is false for S2 (one seat's `Valid`). Past `palw_rcore_plus` the anchor settles only
  when the claim's `basis_k ≥ 2`: at a V1 or coverage licence, or at the first supplementary set that raises an S2
  claim to 2 (SR-10, Q-5). An S2 licence that never upgrades never advances `settled_attempt_finals` or
  `recent_anchor_daas`, so one Sybil full seat cannot move the clock that releases rewards.

**H-1 (heartbeat carriers; normative).** Owner A (fold, C-7), B (miner and relay, P2-9). Test T38. v2's rule, extended: a heartbeat block MUST be able to carry, and the
fold MUST apply at step 3, every conviction-bearing, DA and reporter object (`ObjectiveOffence{PanelFalseValidV2}`,
`ExecutorEquivocation`, `ObjectiveOffence{ExecutorRefuted}`, `DefaultAccused`, `DefaultAccusedHeld`,
`MaterialDisclosedV2`, `ReporterCommitted`, `ReporterRevealed`, `CourtOpened` and court moves where the phase allows).
The heartbeat miner MUST include them; the H2 relay allowance MUST NOT drop a heartbeat for carrying them. Owner A
(fold, C-7), B (miner and relay, P2-9). Tests T38.

### 3.4 L — the seat lock

**L-1 (priced by the recounted set; F4).** Owner A (S), B (M4). Tests T15, T71.

`lock = palw_seat_lock_required_v2(G_res, k') + ⌈100‰ · E_v^max / k'⌉`, with `E_v^max = escrowed_reward − s` and
**`k' = max(basis_k, 2)`**, where `basis_k` is the licensing set's recounted k (Q-3). The door no longer sets k by
name (`palw_door_colluding_signers_v1`, `palw_economic_safety_v1.rs:243`, is replaced past this fence).

| class | lock_3 (V1 quorum) | lock_2 (coverage; the S2 full seat) | Σ over the k load-bearing signers |
|---|---|---|---|
| floor | 106.74 | 160.11 | 320.21 |
| 8k | 306.08 | 459.12 | 918.23 |
| 2M | 22,252.74 | 33,379.11 | 66,758.23 |

v2's `lock_1` (S2 at 320.21 / 918.23 / 66,758.23) is gone: S2 never carries a claim to Final (Q-5), so nothing is
extracted on a lone full seat's word, and that seat locks `lock_2`.

**As armed on the integration line (IA-3, IA-5, IA-7).** `palw_rcore_lock_v1` is the one price.
* **With the vesting rows** (`PALW_RCORE_VESTING_ROWS_LANDED_V1 = true`) it is `palw_rcore_lock_vested_at_cap_v1`:
  L-1 above with the buyback bound at its **cap**, `s_cap = 5% · E` (`palw_model_buyback_slice_v1`), whatever the
  pair's state at the licence. That is `palw_seat_lock_required_v2(G_res − s + s_cap, k') + ⌈100‰ · (E − s_cap) /
  k'⌉`. **Why (the S re-review):** a pair can open between the licence and the `Final`. A lock priced on a closed
  pair's `s = 0` would then be short by about `s / k'`, and the at-cap price does not depend on the pair.
* The table above is L-1 at `s = 0`. `rcore_whole_gain_and_buyback` pins exactly those values as "the residual at
  `s = 0`". The fold's armed price is about `s_cap / k'` higher: the test gives `s` ≈ 160 MSK and ≈ 53 MSK on 8k's
  `lock_3`. The armed values are not tabulated here.
* **Without the rows** (a build with the flag `false`, which kaspad refuses to run on a `palw_rcore_plus` network) it
  is `palw_rcore_lock_unvested_v1`: the whole gain `G = G_res − s + E`, with `s` inside `E` (floor `lock_3` 1,173.69
  per the same test).
* **`k'` is the recount's, not the door's (IA-5).** A claim recorded `Quorum` may carry `basis_k` 2 (an upgraded S2
  claim, SR-2), and it locks `lock_2`.

**L-2 (recount, not a flat divisor).** Owner B (M4). Test T15. A flat `/3` would under-price a coverage licence, which the recount shows
is carried by 2 attesters per segment (§3.12 Q-3).

**L-3 (lifetime; V3S-04).** Owner A (S), B (masks, M4). Tests T44, T66, T73. Written at licence (or at a
supplementary set, SR-10), re-dated at Final, bounded at +6,000 by the second clock, releasable by the escape (exit),
not pruned while its row exists. Past this fence the lock also records the signer's attested mask
(`attested: PalwSegmentMaskV2`, `segments: u16`; a V2 `Valid` records the full mask) for Q-6. Locks never decrease
before Final. **As integrated (IA-5; S-3):** a V1 or V2 `Valid` is recorded as the full cut and a V3 `Valid` as its
own mask, at the licence and through either supplementary door. The recount reads these recorded masks
(`palw_rcore_counted_masks_v1`). A lock written with no mask (`segments == 0`) reads as the seat's **assigned** mask,
never the full cut, so the recount can only under-count, which holds the escrow rather than releasing it.
* **A lock follows its row under DA (V3S-04).** When DA-5 re-keys a row (`expiry_daa` raised when a session opens),
  every live lock of that claim's `Valid` signers is re-dated to the same `expiry_daa` in the same funnel, so the lock
  predicate and V-4(a) stay one predicate over one pair. Without this, a producer that holds its data could answer
  each round at `deadline − 1` and push a post-Final conviction past F + 3,000, where S4 no longer lands: the
  post-Final break-even `q*post` would rise from 0.0900 to 0.4962 on the floor V1 door, 0.0912 to 0.5805 on 8k and
  0.3726 to 9.86 on 2M (13k producer, 130k seats; `v3_numbers_r2.py`, reproducing the verifier).

**L-4 (duty reserves the lock; X8, X9, F14).** Owner A (S). Test T77.
* **At bind**, each seat's duty is `duty_bind = min(max(λ-term, lock_2), ⌊(w + E)/seat_count⌋)`:
  floor 256.07 (λ binds), 8k 459.12 (lock_2 binds), 2M 12,588.76 (the cap binds). In S-SPEC's form
  (`palw_rcore_duty_bind_v1`) the cap is the claim's commitment at bind over `seat_count`: `w + E` for an attempt
  (`rr` = 0), `w + rr` for a free-prompt claim, which is its forfeit, so the ≤ 1 bound holds on the FP lane too.
* **The bound is ≤ 1 for every class:** `seat_count × duty_bind ≤ w + E`, so a withholder never pins more of the
  panel than it forfeits. This replaces v2's 2M-by-name exception with a rule (F14).
* **IA-3's effect.** `duty_bind` reads the fold's armed `lock_2`, which is the at-cap price (L-1). Where `lock_2`
  binds the duty, as on 8k, the duty and the withholding amplification move with it. The figures in this section and
  in §3.8 are at `s = 0` and are not recomputed here. The ≤ 1 bound holds by construction whatever the lock. Under the
  armed price the floor and 8k locks still sit within the duty the bind reserved, so a seat whose room the bind used up
  is still backed and five `Valid`s license through the V1 door with no top-up; SR-6's unbacked path is where the lock
  tops the duty up, the 2M row (T33) (`8e1ce34a`, `rcore_s3_one_ledger`).
* The committed ledger counts `max(duty, live lock)` per `(seat, claim)`. On floor and 8k attempt claims every lock
  (`lock_3`, `lock_2`) is ≤ duty, so every `Valid` is backed by construction. **Not on the FP lane (post-edit 10(c)):**
  `escrowed_reward` is 0 there and the lock is priced on `max(R, rr)` (STATE:9174 at `f8c91f19`), so `lock_2 > duty`;
  L-4b's eligibility covers it. At Final the duty is released and the lock remains.

**L-4b (the lock top-up, generalized; F15, post-edit 10(c)).** Owner A (S). Test T78.
* **Eligibility is `max(duty_bind, lock_2)` for every claim** (S-SPEC §3.3, `palw_rcore_seat_eligibility_v1`): on floor
  and 8k attempts that is L-4's duty (`lock_2 ≤ duty`); on 2M it is this rule; **on FP claims it is `lock_2`, which
  prices `rr`** (`escrowed_reward` is 0 on FP), so an FP `Valid` is backed although its lock exceeds its duty.
* On 2M the lock exceeds the capped duty: `lock_3 − duty` = 9,663.98 and `lock_2 − duty` = 20,790.36 MSK per
  signer, taken from headroom at licence.
* The draw (`palw_panel_v2.rs:702` at `f8c91f19`; the Valid-lock filter `PalwPanelValidLockV1::admits`) requires
  `committed + max(duty_bind, lock_2) ≤ ceiling` (eligibility) for every claim, which on floor and 8k attempts is
  `committed + duty_bind`, while reserving only `duty_bind`.
* **The residual void, named (§4, branch 11).** A seat can still take other duties between bind and licence and
  fail its top-up. With ≥ 3 such seats an honest 2M claim cannot license and, under D7, forfeits at RT#2. Example
  at v3's prices: a 130k seat (ceiling 65,000) takes the 2M duty (12,588.76) and then 114 8k duties (459.12 each),
  which leaves 71.56 MSK of headroom against a 20,790.36 top-up. (The review's v2 example was 211 8k duties at
  306.08 beside a λ-only 2M duty.) Reserving `lock_2` at bind would close it, at an amplification of 2.65 on 2M,
  which F14 forbids. This is the same shape as silent-quorum griefing, which D1 and D7 accept.

`require_panel_lock_eligible` and the draw require `committed + max(duty_bind, lock_2) ≤ ceiling` (post-edit 10(c);
v3 wrote `committed + duty_bind`, which is the same on floor and 8k attempts).

### 3.5 A — one committed-collateral ledger

Unchanged from v2, except where noted.

**A-1.** Owner A (S). Tests T09, T40. `palw_bond_committed_v1(state, bond, now_daa, escaped_depth, window_court)` =
own claims' `palw_claim_commitment_v1(.., now_daa)` + `registration_exposure(bond)` + Σ over `(bond, claim)` of
`max(duty(bond, claim), lock.amount if lock.is_live_v3(...) else 0)`. Duties move out of `reserved_exposure`. The
Σ reads the derived index of duties by seat bond (IMPL-14), not a scan of `panel_duties`. Accuser exposure is **not**
in it (A-6).
* **As S implements it (post-edit 9; S-SPEC §3.1, §9 P2):** duties **stay** in `reserved_exposure` (written as
  `duty_bind`), and `palw_bond_committed_v1` adds only each live lock's excess over its duty, which is the same number
  (own commitments + registration + Σ `max(duty, live lock)`). A-1's value is unchanged; its text "duties move out"
  and the duty index are not what S builds, and §6 row 29 drops the index. v5's exit clause, readiness and the room
  then do not move.

**A-2.** Owner A (S). Test T23. `palw_claim_commitment_v1` and `palw_bond_committed_v1` are the only readers of a bond's commitment.
B-3's gate reads the latter (F19).

**A-3.** Owner A (S). Test T09. `committed ≤ collateral × 500‰` at every gate. `dos_l5_4b` must pass. Past
`palw_rcore_plus` every work gate also keeps A-6's one invariant `committed + accuser ≤ C` (IA-2).

**A-4 (duty; not slashable).** Owner A (S). Tests T77, T09. `palw_panel_seat_exposure_v1` returns L-4's `duty_bind`; the `3 × claim.reserved`
term is retired on t12. The duty is not slashable (`Withheld` abstains; `served_won = false` is retired). It rests
on `palw_unavailable_abstains`, a fence prerequisite.

**A-5.** Owner A (S). Tests T22, T39. The first 50% of any action slash is collectible from uncommitted stake unless earlier slashes consumed
it. `slash_bond` saturates (STATE:13373). What it actually took is the conviction's **collected debit** (R-2).

**A-6 (accusers use the free half; C-6, IMPL-4).** Owner A (S; DA terms in M3). Tests T84, T42, T69.
* **The ledger.** `palw_accuser_exposure_v1(state, bond)` = Σ `exposure` of the bond's open `da_sessions` + Σ its
  refuted exposure held in `da_claims[*].refuted_held` (DA-6) + its court challenger reservations. It is derived
  from rooted maps, never stored twice, and `assert_internal_consistency_v3` re-derives it on load.
* **The check.** v3.1 had an accusation check `committed + accuser_exposure + new ≤ collateral × 1000‰`, with the
  work gates on the 500‰ ceiling alone. **Amended by the S review's M1 (IA-2): one invariant at every gate past
  `palw_rcore_plus`, `committed + accuser ≤ C`**, with `C` the posted collateral and `ceiling = C × 500‰` on t12. There are two gates,
  and one pure function computes both, `palw_rcore_gate_room_of_v1(collateral, ratio_permille, committed, accuser,
  gate)`:
  * **work** (`PalwRcoreGateV1::Work`: a claim, a duty, a lock top-up, an FP commitment): room
    `min(ceiling − committed, C − committed − accuser)`, that is `committed + new ≤ ceiling` **and**
    `committed + accuser + new ≤ C`;
  * **accuser** (`PalwRcoreGateV1::Accuser`: a court, a held dissection, a DA accusation): room
    `C − max(committed, ceiling) − accuser`, that is **`max(committed, 500‰·C) + accuser_exposure + new ≤ C`**.
    Accusers stake only the free half and never the work half's unused room, so a later claim can never be the one
    that breaks the invariant.

  A seat full of locks can still accuse on its free half. Every reader computes one number. The fold reads it through
  `gate_room` and `palw_rcore_gate_room_v1`, whose wrappers are `palw_bond_headroom_v1` (the work gates of
  `apply_attempt`, admission item 8, the FP lane, the bind, the licence top-up and the draw) and
  `palw_accuser_room_v1`. Admission (`palw_admission_v2.rs`), the draw (`palw_panel_v2.rs`) and the RPC call the same
  function. The producer's facts carry `committed` and `accuser_exposure`, and `has_committed_room` applies the work
  gate's inequality; `ready_to_produce_v3` and kaspad's P6 read it (B-4, IA-15). Tests:
  `the_work_and_accuser_gates_keep_one_invariant`, `rcore_one_invariant`, and M3's DA accusation refused past the free
  half. Past this fence the court challenger reservation leaves `reserved_exposure` (today
  `reserve_accuser_exposure_v2` adds it there and checks it against 500‰) and joins this ledger, so SR-3's load-time
  re-derivation of `reserved_exposure` from claims alone stays exact.
* **Court time can debit up to twice the reserve (IA-4; a note on existing pricing, not a change).** A-6 reserves a
  court's `claim.reserved` on the challenger. A challenger-side close takes that reservation (`slash_seat`, capped at
  `min_collateral_sompi`, in `rearm_after_challenger_side_close`), and the deep fence's C-03 court-time charge
  (`charge_court_time_v1`) takes up to `claim.reserved` more, in proportion to how long the session ran. A losing
  challenger can therefore be debited up to 2× what A-6 reserved. The second part comes from whatever collateral the
  bond holds (`slash_bond` saturates), not from reserved room. The same court-time charge falls on a losing executor
  beside its verdict tier.
* **Exit.** B-3's gate holds while `accuser_exposure > 0`.
* Reporter commitments reserve nothing: they are capped per bond (R-3), not priced.
