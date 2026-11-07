> **Archived verbatim from ADR-0152 v3.1** (branch `docs/adr-0152-v31-postedits` at `9ed1adced`,
> lines 2516–3095 of `docs/adr/0152-account-stake-staged-reserve-and-vested-rewards.md`): §3.12 Q (quorum counting), §3.13 DL (deadline function), §3.14 SW (stake-weighted panel draw).
> Not normative. The rules are in [spec/palw](../../../../spec/palw/00-index.md); the decision record is
> [ADR-0152](../../../../adr/0152-account-stake-staged-reserve-and-vested-rewards.md); the reading guide is [README.md](README.md).

### 3.12 Q — quorum counting and sampler roles (F4, D2)

Owner **B** (the fold rules and the kaspad samplers), M4, after M3. The audit session reviews the fold half.
Tests T70–T74, T72b, T45, T54e.

**Q-1 (verdicts; C1, IMPL-11).** Owner B (M4; the variant declared in S). Tests T70, T68.
`PalwReceiptVerdictV2::Sampled`, appended (Borsh index 3; `palw_receipt_message_v2` tag byte 4). Past
`palw_rcore_plus`:
* `Valid` in a V2 receipt (the V1 object) attests a **full replay** of the job. Node policy: a seat signs a V2 `Valid`
  only after a full replay.
* `Valid` in a V3 receipt attests an S1 **replay** of exactly the segments in its mask (the full seat's mask is full).
* `Sampled` attests that the seat held its S3 sites and recomputed them with no mismatch. It is audit, not
  verification:
  * it never counts toward a quorum or a coverage count;
  * it takes no lock and carries no liability, and `PanelFalseValidV2` never convicts it (it is not `Valid`);
  * it does **not** count as served for SR-1 condition 2 (C1): its sites are public at bind, so it proves nothing
    about the rest of the delivery;
  * it is credited for seat pay as a `Valid` is today (§9.1 Q3);
  * **as integrated (IA-5):** it latches `claim.rcore.unserved_seen` like `Unavailable` and `Incapable`, at the licence
    (`staged_licence_v1`'s `Sampled` arm: no mask, no served bit) and through SR-10's door, so a set it rides in never
    releases the escrow.
* **The answer it maps to (IMPL-11).** `palw_seat_verdicts_of_v2` maps every verdict to `PalwSeatAnswerV2`, which drives
  quorum validation, `slash_dissenting_seats`, `credit_seat_receipts` and ADR-0147's outsider veto. Past this fence it
  gains an appended fourth answer `PalwSeatAnswerV2::Sampled`: it is credited, counts in no quorum, is never
  dissent-slashed, is not served, and **does not satisfy the ADR-0147 outsider veto**
  (`palw_licence_names_its_outsider_v1` requires seat 0 to answer `Served`, which only a `Valid` gives). A registered
  class whose outsider only sampled therefore does not license on that set.
* Below the fence `Sampled` is refused (processor and fold), so fold **behaviour** is unchanged on other networks.
  Their record **encodings** and state roots still move with v22 (the state version and appended record fields,
  §6, IMPL-6); no shipped network other than t12 runs this binary's state.
* An S3 seat that finds a mismatch files a refutation (PANEL's `Faulted` → the capture-arm `ExecutorRefuted`, P2-8),
  not a receipt.

**Q-2 (the doors stay enabled).** Owner B (M4). Test T71.
* V1 `ReceiptLicensed`: ≥ 3 `Valid` V2 receipts plus the outsider; unchanged.
* Coverage `ReceiptLicensedV2`: a quorum of 3 `Valid` and every segment attested by ≥ 2 `Valid` masks
  (`PALW_VERIFICATION_V2_ATTESTATIONS_PER_SEGMENT`). `Sampled` counts in neither. In the shipped geometry (one full
  seat, four partial seats, one segment each) that is all five seats signing `Valid`.
* Optimistic `OptimisticLicensed`: the full seat's `Valid` alone, as a fast path (Q-5).
* ShardPart: **dormant on t12** (Q-3).

**Q-3 (the recount; C10, V3S-13, IMPL-18(a)).** Owner B (M4). Tests T71, T15.
`palw_receipt_set_basis_k_v1(receipts_with_masks, segments) -> u8` = `min(3, min over segments s of #distinct counted
Valid signers whose mask covers s)`, where a V2 `Valid` carries the full mask and a V3 `Valid` its own mask. One
function for every door and for mixed supplementary sets (SR-10):
* V1 (all V2): `min(3, #Valid)`;
* Coverage: **2** in the shipped geometry: the full seat plus the unique partial holder (`palw_verification_v2.rs`
  module doc);
* Optimistic: 1; an upgrade adds the partial holders' V3 `Valid`s or any seat's V2 full-replay `Valid` (Q-5).

It is recorded in `claim.rcore.basis_k` at licence. A supplementary set may raise it, never lower it. **The door
recorded beside it does not determine it (IA-5):** an upgraded S2 claim is recorded Quorum or Coverage with its
recounted `basis_k ≥ 2`, 2 when one seat is added (Q-5). The V2 door's exact recount `min(3, basis_k + new)` agrees
with Q-3 over the recorded masks (t71).
**Final requires `basis_k ≥ PALW_RCORE_FINAL_BASIS_K_V1` = 2.**

**`palw_shard_licensing` is dormant on t12 (verified):** `palw_t12_arm_every_rule_from_genesis` leaves it commented out
(`// params.palw_shard_licensing = Some(at);`, `params.rs:15357` at `f8c91f19`) with the reason that
`validate_palw_v2` refuses it beside `palw_admission_independence` (ADR-0147), and asserts
`params.palw_shard_licensing.is_none()`. So ShardPart and the "shard" rows of §4.3 do not apply on t12. If a later
fence arms it, its `quorum_per_shard` must be ≥ 2; T71 asserts the dormancy now and pins that rule for then.

This is what D2's "recount k" means. `palw_door_colluding_signers_v1` priced coverage at 3 ("the quorum is the bound
the door guarantees", `palw_economic_safety_v1.rs:243`). That is false when honest partial seats sign `Valid` for
segments that do not hold the lie: a lie in segment i needs only the full seat and segment i's holder. D4's "several
independent pieces of evidence" is this count: at least two independent replays of every part of the job. Raising the
coverage basis to 3 needs a t12 assignment that gives each partial seat two segments (deferred, §9.2; v3.1 took the stake-weighted draw instead, §3.14).

**Q-4 (lock price).** Owner B (M4). Tests T15, T73. Each `Valid` signer counted at licence or at a supplementary set
locks `lock_{max(basis_k, 2)}` (L-1) and records its attested mask (L-3). A signer already locked keeps its lock.

**Q-5 (S2 is a fast path, never the basis for Final; V3S-01, IMPL-1, IMPL-2, C5).** Owner B (M4), fold half reviewed
by A. Tests T72, T72b, T37.
* An `OptimisticLicensed` claim is `ReceiptLicensed` with `basis_k = 1`, escrow held (SR-1 cond. 1), and replay still
  charged (T-2(c)). Its licence does not settle the anchor (V-8).
* **What an S2 licence carries (post-edit 13).** After the licence-stall fix the optimistic builder takes the full
  seat's `Valid` first and at most one partial seat's, so an optimistic licence credits at most 2 seats and recounts to
  `basis_k = 1`, unless all five validated (then all five ride, never three or four, and the coverage door normally
  takes that set first; `palw_panel_v2.rs:1886` at `c8652a97`, licence-stall report Fix 1). On the launch candidate t12
  has **no supplementary path** for the rest: kaspad seats sign only V3
  receipts under Verification V2, and the only door takes V2 receipts. Both upgrade routes below arrive with M4
  (SR-10's V3 door, B; Q-7's full-replay V2 `Valid` policy).
* **Upgrade.** Supplementary receipts ride SR-10's V3 door (partial seats' V3 receipts, any verdict) or the V2 door
  (full-replay V2 `Valid`s). They are recounted with the licensing receipts by Q-3. When the recount reaches
  `basis_k ≥ 2`:
  * the claim's door is recorded as **Coverage** if a counted mask is partial, else **Quorum** (IA-5: so a Quorum record
    can carry `basis_k` 2);
  * newly counted signers lock, with their masks;
  * the anchor settles (V-8);
  * SR-1b may flip the release.
* **Honest seats can complete it (V3S-01).** A V2 full-replay `Valid` from **any** seat covers every segment, so one
  silent, `Sampled` or late partial seat cannot block the upgrade: any other honest seat full-replays and signs a V2
  `Valid` (Q-7). The upgrade is blocked only if all four non-full seats withhold; with t11's measured 30% per-seat
  offline rate that is 0.3⁴ = 0.81% of honest S2 claims (model), and they redraw (next bullet).
* **Gate (DL-1).** A claim with `basis_k < 2` has the deadline `max(licensed_daa + window_challenge_at, bound_daa +
  window_receipt + 1)`. If `basis_k < 2` still holds then:
  * **on the claim's first panel, it redraws** (V3S-01), exactly as a first `ReceiptTimeout` would: phase `Provisional`
    with `rebound_daa = now`, the S2 signer's lock released and its credit dropped (the claim never reached Final),
    duties released, the commitment unchanged (`w + esc + rr` throughout, so SR-4 is not engaged), open DA sessions
    kept (DA-5);
  * **on the second panel, it voids `NotReplayBacked`** (an appended `PalwVoidReasonV2`), charged as in SR-5: S0′ at
    launch, S0 after X10, S0′ always for C7.
* **The upgrade's finalization deadline** is `max(licensed_daa + window_challenge_at, U)` for an upgrade in block U,
  today's court-clear pattern, which `rebuild_deadline_index_v2`'s `max(floor, last_daa)` reproduces. The draft's
  `upgrade_daa + 1` could not be rebuilt (`upgrade_daa` is stored nowhere) (IMPL-2).

**Q-6 (whom a conviction binds; D2's exclusion; v3.1: the audit's liability rule).** Owner A (the rule, M1), B (the
`Sampled` exclusion and the lock masks, M4). Tests T46b, T46c, T73.
* **`PanelFalseValidV2` liability is the audit's (J-3, SPEC §3.3 step 9):** a `Full` receipt is always liable; a
  `Segmented` receipt is liable by its (authentic, assigned) mask and the fault's site. Sites: `Leaf(l)` for
  `StepArithmetic` and `StepStructural{StepTile|KvChunk}`; `Whole` for `Shape`, `Checkpoint*`, `ForgedOutput`,
  `OutputMismatch`, `ProducerWithholding` and `CourtFraud`; for `IdentityMismatch` (J1/J2/J3/J5a/J5b) and
  `PromptNotAnchored` (13), `AnyValid` **only if** SEAT-S1, SEAT-S3 and SEAT-S4 (the addendum's seat fixes S-1, S-3
  and S-4, §7.1 "Seat fixes") ship and T18p-M is GREEN before the regenesis, else `Whole` (a one-line table flip);
  `IdentityMismatch` on J4/J6/J7 is `Whole`; 11 and 12 are `Whole` (a partial seat never sees the logits trace or the
  argmax) (ADDENDUM §4-bis.7). v3's located `ForgedOutput` (a position-to-leaf mapping) is withdrawn: `ForgedOutput`
  is `Whole`, so only full-mask signers answer for it.
* **A DA default** (DA-7) binds the covering signers of its unanswered units (C7) by the masks recorded in their locks
  (L-3), not every locked signer, under the same (seat, claim) key (N9).
* **`Sampled` is never liable (B, M4).** A sampler signs `Sampled`, which is not `Valid`, so the adjudicator's verdict
  check refuses it; it holds no lock. This is what SPEC §5 item 3 asks F4 to guarantee.
* **Lock masks (B, M4).** `lock_valid_receipts` records each `Valid` signer's assigned mask (L-3); it equals the
  receipt's mask for every licensed receipt because coverage licences require the assigned mask, so the two liability
  sources cannot disagree. T73 asserts that equality.
* **R-6 under the rule.** A `Leaf` fault convicts at least the `basis_k` signers that attested the faulty segment (the
  full seat and the segment's holder on coverage; every V1 signer). A `Whole` fault on a coverage licence convicts only
  the full seat (j = 1 of k = 2); R-6's j = 1 column shows that floor (3,344.95 ≥ 0.12) and 8k (3,641.24 ≥ 543.77) still
  hold, and 2M is already expected-FAIL. On V1 every signer is full, so j = k = 3.
* The evidence type `PalwPanelFalseValidEvidenceV2` is the audit's (M1); v3's M4 wire item (`valid_receipt:
  PalwSeatReceiptV3, mask`) is withdrawn.

**Q-7 (node roles; Phase 2 and F4 kaspad).** Owner B (M4, P2-6). Tests T54e, T45, T72b, T18p-M.
* **SEAT-R (post-edit 3; B's, a hard ship rule).** **SEAT-R ships in the same binary as F2's fence**
  (`palw_offence_attribution`). Without it F2, as committed, slashes honest kaspad seats: the material arms sign
  `Valid` without checking any arithmetic, a failed full replay falls through to the interval arm, and every kind-4
  conviction cascades onto full-mask signers through F2's `CourtFraud` contradiction at `Whole`. Under
  `palw_offence_attribution_at && palw_verification_v2_at` (ADDENDUM §4-bis.10 has the `palw_panel.rs` line
  references):
  * a **full-mask `Valid` comes only from the attempt replay or the free-prompt replay**;
  * **a replay mismatch is terminal** (no fall-through to another arm);
  * the **material, interval and capture-sample arms are evidence-only**: they still persist and pool material, and on
    a mismatch the seat keeps the bytes as evidence, but they are no longer `Valid` exits;
  * **partial seats sign only from the S1 resume** (attempt and free-prompt);
  * **SEAT-S3** (the addendum's S-3, shipped inside SEAT-R): an S3 sampler uses a pooled capture only after
    `verify_material(bytes, full claim roots + anchor) == Matches`;
  * a source-structure test pins the arms, and T18p-M (every drill fault gives no `Valid` from any route; honest runs
    get `Valid` only by replay or S1) gates it and decides the `AnyValid` sites (Q-6).
  SEAT-0 (the seat-side material check, live-eligible now) is the audit's and is patched onto the live fleet
  separately (§7.1 M0); SEAT-R acts only under the fence, so it matters only in the launch binary.
* The full seat replays whole and signs `Valid` with the full mask.
* A partial seat replays its S1 segments and signs `Valid` only after the replay. If S3 is armed it also samples its
  sites. If only the sampling completed by the receipt deadline, it signs `Sampled` (which holds the escrow, C1).
* **A stuck S2 claim (V3S-01).** On the floor and 8k (replays take minutes, precondition iii), a seat that sees an S2
  licence without an upgrade by `bound_daa + window_receipt − 60` full-replays and signs a V2 `Valid`, carried by the
  V2 door. Not on C7 (2M).
* **An unserved seat files (P2-6).** A seat whose fetch has failed files `Unavailable` and a `DefaultAccused` before its
  receipt window closes, and at once when a licence lands on a claim that did not serve it (DA-6, §3.8).
* The S3 path (PANEL:8080-8117 at `e93be0f2`) returns a new `PalwV2SeatPathV1::Sampled` instead of `Attested(mask)`;
  `Attested` is kept for a completed S1 replay.
* The collector offers a set with `basis_k ≥ 2` before S2, and S2 only when no such set is on hand (X22).

### 3.13 DL — one deadline function (IMPL-2)

**DL-1.** Owner A (S; the DA rows in M3; the Q-5 rows reviewed by B). Tests T40, T67, T72, T72b, T02c.
Past `palw_rcore_plus`, `palw_rcore_deadline_v1(params, claim, panel, da_claim, open_courts, last_daa) ->
Option<u64>` is a pure function of rooted data and the last point, and it is the **only** source of a claim's deadline:
the arm sites, `rebuild_deadline_index_v2`, `expected_deadline` and `assert_deadline_consistency` all call it. Today's
consistency check requires an exact `PanelBound` deadline and some stored deadline for every `ReceiptLicensed` claim
without an open court, and the rebuild re-inserts both, so any DA pause or Q-5 re-arm not expressed here would fail
`CarriageInconsistent` on restart or IBD.

| claim | deadline |
|---|---|
| non-terminal, with an open **seat** DA session | none (DA-5) |
| `Provisional` after a redraw | as today, from `rebound_daa` (shifted by DA-5) |
| `PanelBound` | `bound_daa + window_receipt` (`bound_daa` shifted by DA-5) |
| `ReceiptLicensed`, open court | none (as today) |
| `ReceiptLicensed`, `basis_k ≥ 2` | `max(licensed_daa + window_challenge_at, last_daa)` (today's court-clear pattern; an upgrade in block U is covered by `last_daa = U`) |
| `ReceiptLicensed`, `basis_k < 2` | `max(licensed_daa + window_challenge_at, bound_daa + window_receipt + 1, last_daa)` (the Q-5 gate) |
| terminal, any DA session open | none (retirement deferred, DA-5) |
| `Voided{BindTimeout}` free-prompt on its abandon hold | `voided_daa + fp_abandon_hold_daa` (as today, SR-1) |
| terminal | `max(terminal_daa + claim_retirement_daa, da_claim.last_closed_daa + 1)` |

* **The bind deadline under SW-8 (post-edits).** Past `palw_rcore_plus` a panel binds only in its anchor block, so an
  unbound `Provisional` claim (first panel or after a redraw) is due `BindTimeout` once its anchor block has passed
  without its binding. The exact DAA at which the sweep voids it is M4's to fix in DL-1, with T89 and T93, so that the
  rebuild and `assert_deadline_consistency` reproduce it (UNVERIFIED here). **Fixed by the integration (IA-1a,
  IA-1c):** the void DAA is **the anchor block's own DAA**. The anchor block is the first attempt block at or past
  the slot. Step 4c (`palw_void_claims_unbound_in_their_anchor_block_v1`) runs after the block's own and merged work
  and voids there every `Provisional` claim whose slot the block has reached, by `bind_timeout_reason` (`BindTimeout`,
  or `NoCapablePanel` for a rowed class that cannot seat a panel). No row is added: the index still holds `bind_base
  + window_bind` for a `Provisional` claim as the backstop, reached only when no attempt block arrives in the window.
  A voided claim's deadlines are derived from its record as before, so the rebuild and `assert_deadline_consistency`
  reproduce the transition unchanged (`sw8_the_anchor_block_voids_a_claim_it_does_not_bind`).
* **Composition.** A court clearing, a DA close, an upgrade and a redraw each just re-evaluate DL-1; a court clearing
  during a seat DA pause therefore arms nothing, and the DA close arms from the shifted anchor.
* **Why `max(floor, last_daa)` is exact at rest.** As in today's rebuild proof: an unswept licensed claim has
  `floor ≥ last_daa`; a claim re-armed at block C with no later block has `last_daa = C`; after a DA close at C the
  shifted floor is ≥ C because the pause began before the floor was swept.
* T40 restarts mid-session and mid-gate on `PanelBound`, `ReceiptLicensed` (both `basis_k` cases) and Final claims,
  and mid-abandon-hold, and requires equal roots and no `CarriageInconsistent`.

### 3.14 SW — the stake-weighted panel draw (Q4, v3.1; body synced by the post-edits)

Owner **B** (M4, beside F4); the audit session reviews the acceptance and fold half. Tests T06, T85–T94. Fence:
`palw_rcore_plus`. Figures: `v3calc/v31_stake_draw.py` and, for the review's figures, `v3calc/v31_review_numbers.py`
(the reviewer's independent scripts reproduce every figure they share with these). This section carries the v3.1
review (rows SW, SW-A1…SW-A6, R2, R5 of "v3.1 changes") into the rules; the v3.1 draft's text is in
`0152-v3.1-draft-snapshot.md`.

**Code anchors (R5).** At `f1dfb33b` (`feat/t12-rcore-stake-draw`: the launch candidate `c8652a97` plus P7); DRAW lines
at `0533e1de` in brackets. STATE lines are the same at `0533e1de`, `c8652a97` and `f1dfb33b`.

| what | where |
|---|---|
| `PalwPanelDrawPolicyV1` (stays `Copy` and `Default`) | DRAW:94 [:94] |
| its field `stake: Option<PalwPanelStakeDrawV1>` (P7; `None` at every resolver and literal) | DRAW:112 [new] |
| `PALW_DRAW_WEIGHT_CAP_MSK_V1: u64 = 1_000_000` (SW-2) | DRAW:118 [new] |
| `PALW_DRAW_ELIGIBLE_FLOOR_PERMILLE_V1: u16 = 875` (SW-10) | DRAW:124 [new] |
| `PalwPanelStakeDrawV1 { weight_cap_msk: u64, eligible_floor_permille: u16 }` (`Clone, Copy`; not Borsh, not state) and `PalwPanelStakeDrawV1::V1` | DRAW:137, :146 [new] |
| the one resolver, `palw_panel_draw_policy_at(anchor_daa)`: sets `stake: None` today; M4 sets `Some(PalwPanelStakeDrawV1::V1)` iff `palw_rcore_plus` is active at the anchor DAA | PROC:8991, the field at :9024 [PROC:8971] |
| `PalwPanelValidLockV1` (the Valid-lock filter; S-SPEC §3.3 adds the one-ledger eligibility to it, L-4b) | DRAW:162 [:121] |
| `PalwPanelV2Error::InsufficientEligibleBonds`; SW-10 appends `InsufficientEligibleStake` | DRAW:344 [:303] |
| `derive_panel_v2_with_policy` (the class draw) | DRAW:836 [:782] |
| `palw_panel_outsider_seat_v1` (ADR-0147's outsider) | DRAW:993 [:939] |
| `palw_admission_jury_v1` (**unchanged**, SW-5) and its fold caller `admission_jury_seated` | DRAW:1045 [:991]; STATE:12204 (the call at :12242) |
| `palw_panel_operator_entries_under_v1`, `palw_panel_operator_lottery_of_v1` (ADR-0130's lottery, which the key race replaces past the fence) | DRAW:1120, :1166 [:1066, :1112] |
| `validate_panel_bound_v2_with_policy` (acceptance) | DRAW:1312 [:1258] |
| `palw_segment_assignment_v2` (the S1 roles; unchanged, SW-6) | `palw_verification_v2.rs:130` |
| `panel_room_v1`, `panel_rate_v1`, `model_registry_ready_seats` (SW-9's `ready_eff`) | STATE:9431, :9523, :9551 |
| one operator, one bond: `DuplicateOperator` under `Params::palw_operator_id_unique` | STATE:17278; `params.rs:1351` |
| **integrated (`f422df49`, on `rcore/int-2`; by symbol):** the resolver sets `stake = Some(PalwPanelStakeDrawV1::V1)` iff `palw_rcore_plus` is active at the anchor DAA | `palw_panel_draw_policy_at` (PROC) |
| the anchor lane, one predicate for the walk and the fold (IA-1a) | `palw_block_may_anchor_a_panel_v1`, `palw_sw8_anchor_delay_for` (PROC); `PalwTransitionExtrasV1::sw8_anchor_delay` |
| the one-state derivation of a block's bindings | `palw_v2_derived_panel_bindings_on_one_state_v1` (PROC) |
| step 4c, the void in the anchor block (IA-1c) | `palw_claims_provisional_past_their_anchor_slot_v1`, `palw_void_claims_unbound_in_their_anchor_block_v1` (STATE) |
| SW-10's executor term (IA-1b) | `palw_panel_stake_executor_bonds_judging_v1`, `palw_panel_stake_race_with_v1` (DRAW) |
| SW-9's `ready_eff` | `palw_panel_ready_eff_v1` (DRAW), read by the fold, `panel_rate_v1` / `panel_room_v1` and op 186 |

**What it replaces.** Past ADR-0124's panel economy the draw gives each eligible operator one lottery entry (ADR-0130,
`palw_panel_operator_lottery_of_v1`): the ticket `H(operator-ticket domain ‖ anchor ‖ claim ‖ operator_id)` whatever
the operator's stake, with `PalwPanelDrawPolicyV1::weighted` (C-02's bucketed sub-tickets) ignored there by design
(ADR-0124 D5). On t12 an operator id backs exactly one bond and a registration proves possession of its operator key
(`palw_operator_id_unique`, armed at genesis; B-4 as corrected by SW-A3), but a fresh key costs nothing, so m Sybil
operators at the 130,000 MSK seat floor are m full entries for m floor bonds. v3 priced the resulting collusion
residual at 20 operators (2.60M MSK). The operator decided (Q4) to reverse D5 on t12 before launch.

**SW-1 (scope and fence).** Owner B. Tests T88, T24.
* Past `palw_rcore_plus`, **two** draws are stake-weighted by SW-2…SW-3: the flat class draw
  (`derive_panel_v2_with_policy`) and ADR-0147's outsider seat (`palw_panel_outsider_seat_v1`). **ADR-0147's admission
  jury is not** (SW-5, SW-A4): `palw_admission_jury_v1` and `admission_jury_seated` never read the stake terms.
* `PalwPanelDrawPolicyV1` gains `stake: Option<PalwPanelStakeDrawV1>` (P7, committed at `f1dfb33b`; the type promised
  to the audit's S). It is not Borsh and not state, and the struct stays `Copy` and `Default`. It carries the **terms**,
  not the weights: `PalwPanelStakeDrawV1::V1` = `{ weight_cap_msk: 1,000,000, eligible_floor_permille: 875 }`. The
  draw reads each operator's posted collateral off the state it already draws on. The processor's one resolver,
  `palw_panel_draw_policy_at`, resolves it **at the claim's anchor**, like every other policy field: `Some(V1)` iff
  `palw_rcore_plus` is active at the anchor DAA. M4 fills it; until then every resolver and literal sets `None`, so
  every draw is byte-identical to `c8652a97`.
* **`None` is today's draw byte for byte** (behaviour and panels): ADR-0130's operator lottery and ADR-0147's outsider
  ticket. That covers testnet-11, devnet and mainnet. No params field is added (the draw rides `palw_rcore_plus`), so
  the draw moves no fingerprint beyond that fence's own. (Post-edit 10(b): v22 itself still moves record encodings on
  every network and re-pins the V2 params id once; that is the regenesis's, not the draw's.) T88 runs both draws over a
  corpus of states and asserts that `stake: None` reproduces today's panels exactly.
* The stratified (shard) draw is dormant on t12 (Q-3) and stays unweighted.
* **Unchanged:** which bonds are eligible (the floor, the one-ledger headroom, maturity and the registered-before-anchor
  cut, the executor exclusions, capability and readiness, the route-matrix Valid-lock filter, the outsider's registrant
  exclusion); one seat per operator; the canonical order (outsider first, then key order); the redraw anchor
  (`bind_base_daa`); and the S1 assignment (SW-6). **Added:** the one state and bind-only-in-the-anchor-block rule
  (SW-8) and the eligible-stake floor (SW-10).

**SW-2 (the weight).** Owner B. Tests T87, T93.
* An operator's weight is the **posted collateral of its one bond**, in whole MSK, **capped at
  `PALW_DRAW_WEIGHT_CAP_MSK_V1`** = 1,000,000:
  `W(op) = min(weight_cap_msk, ⌊bond.collateral / SOMPI_PER_MSK⌋)`. The bond is the operator's eligible bond for this
  claim (for the outsider, in the base-class population minus the registrant). Every eligible bond is at or above the
  seat floor (130,000 MSK), so `W ≥ 1`.
* **One operator, one bond (SW-A3).** On t12 `palw_operator_id_unique` is armed at genesis, so an operator id backs one
  bond, ever (`DuplicateOperator`, STATE:17278). The draw's safety needs it (SW-A3), so it is a §6 prerequisite of
  `palw_rcore_plus`, with a T24 negative case. v3.1's draft wording (an operator's weight sums its eligible bonds) is
  withdrawn.
* **The one ledger decides eligibility only (post-edit 6 superseded).** The one ledger (`palw_bond_committed_v1`, from
  which S-SPEC §2 derives `palw_bond_free_slashable_v1` and the headroom) and the Valid-lock filter decide **whether**
  the bond is drawn; posted collateral decides **how often**. Only a slash lowers a weight; nothing
  raises one after the anchor (ADR-0147's registered-before-anchor cut). Post-edit 6's summed free-stake weight map is
  not taken.
* **The cap (SW-A5, SW-A6).** 1,000,000 MSK sits just above a genesis seat (939,063 MSK), so it touches neither the
  genesis seats nor floor-sized Sybils, and an attacker that already splits at the seat floor (its best split, SW-7)
  loses nothing to it. It exists because the threshold depends on the honest **weight vector**, not its sum (§4.3):
  100M as one operator raises the design-point threshold to 73.19M uncapped (the draft) and to 19.76M capped, against
  258.70M as 100 operators at the cap. Above the cap an operator gains nothing by staying whole, so an honest holder
  above 1,000,000 MSK registers operators at or below it. The cap also bounds the room lever of one heavy ready
  operator (SW-9). The draft's "no relative cap" text is withdrawn: this is an absolute cap, not a share of ΣW.
* **Arithmetic bound.** `W ≤ 1,000,000 < 2^20` and `L ≤ 2^70` (SW-3), so the key products are below `2^90` in u128.
  The draft's `2^40` MSK bound (`PALW_DRAW_WEIGHT_MAX_MSK_V1`) is dropped (§6 row 30).
* **Why posted collateral and not free stake.** The alternative is the operator's free slashable stake under the one
  ledger. It is rejected for two reasons. (The draft's first reason, "it moves after the seed", is **withdrawn**,
  SW-A2: the panel is derived and bound in its anchor block on one state, SW-8, so nothing done after the anchor moves
  a key under either weight.)
  1. **It pays idleness.** A Sybil that never signs keeps weight `C`. An honest seat that works carries locks and drops
     toward `C/2` (the 500‰ ceiling bounds it there). At 1 claim/DAA on the coverage door the genesis seats sit about
     67% free, so the attack threshold would fall from 17.29M to 11.70M MSK, and to 8.84M at the ceiling.
  2. **It buys nothing.** What a conviction on this claim can take from a seat does not depend on its commitments
     elsewhere. The lock for this claim is reserved separately, and the action tier `min(25%·C, 3G)` is collected from
     the uncommitted half, which the 500‰ ceiling keeps at 50% of `C` or more (A-3, A-5).

**SW-3 (the key and the sample).** Owner B. Tests T85, T86, T89.
* For each operator entry, `u` is the first 8 bytes (little-endian) of `H(domain ‖ anchor_block ‖ claim_id ‖ operator_id)`,
  with a new domain for each weighted draw:
  * the class seats: `misaka-palw/panel-v2/stake-ticket/v1`;
  * the outsider: `misaka-palw/panel-v2/stake-outsider-ticket/v1`.

  These are hashing domains, not ML-DSA contexts. They are appended to `PALW_PANEL_V2_ALL_DOMAINS`, which only test-only
  uniqueness lists read. (The draft's jury domain is dropped with the weighted jury, SW-5.)
* `L = palw_draw_neg_log2_q64_v1(u)` is `−log2((u + 1) / 2^64)` in Q64.64, computed by this integer routine (u128 only;
  normative, with golden vectors in T85):

  ```
  m    = u + 1                                              // 1 ..= 2^64, as u128
  n    = 127 − m.leading_zeros()                            // floor(log2 m), 0 ..= 64
  y    = if n >= 63 { m >> (n − 63) } else { m << (63 − n) } // Q63, 2^63 <= y < 2^64
  frac = 0
  for i in 0..64 { y = (y * y) >> 63; if y >= 2^64 { y >>= 1; frac |= 1 << (63 − i) } }
  L    = (64 << 64) − ((n << 64) | frac)
  ```

  Because `y < 2^64`, `y * y < 2^128`. The routine is monotone in `u`. Its error against a float `log2` is below
  4 × 10⁻¹⁵ over 20,008 inputs. Only its determinism is consensus; its accuracy only shapes the distribution.
* Operator i's key is the ratio `L_i / W_i`, compared without division: i sorts before j iff `L_i · W_j < L_j · W_i` in
  u128 (`L ≤ 2^70`, `W ≤ 1,000,000`). Ties go by `operator_id`, ascending.
* The panel is the `needed` operators with the smallest keys, in key order, with the outsider first when there is one,
  as today. Fewer eligible operators than `needed` is `InsufficientEligibleBonds`, as today; eligible weight below
  875‰ of the base weight is `InsufficientEligibleStake` (SW-10). The outsider is the smallest outsider key.
* **What this is.** `−log2 u` is an exponential variable, so the keys run an exponential race. The first `needed` keys
  are a weighted sample **without replacement**, distributed exactly as successive sampling: each seat is drawn in
  proportion to weight among the operators not yet seated. Each operator's key depends only on the seed, its own id and
  its own weight, so adding or removing any other operator never changes it. A walk over cumulative weight intervals
  would not have that property.

**SW-4 (the genesis seats and the floors).** Owner B. Tests T86, T92.
* The eight genesis seats weigh 939,063 MSK each (7,512,504 MSK in all), below the cap. A floor-sized Sybil operator
  weighs 130,000 MSK, 13.8% of a genesis seat. With only the genesis seats eligible, the draw is uniform among them, as
  today.
* Beside 130k operators the per-MSK seat rate is nearly flat. A 130k operator sits 1.35, 1.22 and 1.11 times as often
  per MSK as a genesis seat when 0, 2.60M and 10.01M of 130k operators stand beside it; under the uniform draw the
  figure is 7.22. Successive sampling slightly favours small weights, and SW-7 prices that residual split incentive.
* The seat floor (130,000 MSK) is unchanged, and it is what bounds splitting: an operator needs one eligible bond at the
  floor, so stake S makes at most `⌊S / 130,000⌋` operators.
* The 858M community allocation (with the two 100M rows) and the operator's main wallet are not bonds at genesis. They
  enter the draw only when they are registered, and only for claims anchored after the registration (ADR-0147's cut).

**SW-5 (the outsider, and the jury that stays ADR-0147's).** Owner B. Tests T90, T89.
* **The outsider seat** of a bought class's claim (ADR-0147) is the smallest stake-outsider key over the base-class
  population, minus the registrant's bond and operator, after the Valid-lock filter. The outsider is the one seat meant
  to be independent of the class's own population. Left uniform, it would be the cheapest seat on the panel to capture.
* **The admission jury stays ADR-0147's, past the fence and below it (SW-A4).** `palw_admission_jury_v1` and
  `admission_jury_seated` are unchanged: one uniform ticket per juror operator. Q4 decided the panel draw; the jury
  decides admission, and weighting it has a cost Q4 did not weigh: it refuses honest classes the genesis seats do not
  load. Per audit (5-seat jury, majority of 3 ready; `v31_review_numbers.py`):

  | an honest class held by (genesis seats not ready) | admitted, weighted jury | admitted, ADR-0147's jury (kept) |
  |---|---|---|
  | 20 × 130,000 MSK ready operators | 0.1325 (about 8 audits of 100 DAA) | 0.8769 (1.14) |
  | 40 × 130,000 | 0.3858 (3) | 0.9728 (1.03) |
  | 20 × 13,000 | 0.0005 (2,126) | 0.8769 (1.14) |
  | 100 × 13,000 | 0.0350 (29) | 0.9974 (1.00) |

  Weighted, one audit admits with probability ≥ 0.5 only with about 6.63M (51 × 130k) of ready non-genesis stake.
* **The kept residual.** Under ADR-0147's jury a registrant's 40 Sybil operators of 13,000 MSK that hold its class win
  a jury majority against the eight genesis seats with probability 0.9728 per audit (about 1.03 audits; a weighted
  jury would have made it 0.0034, about 291 audits). This is ADR-0147's existing hole, and its reach is bounded: every
  claim of the bought class seats the **stake-weighted** outsider, and without the outsider's `Valid` it does not
  license (Q-1's outsider veto). The weighted jury is deferred with this trade-off (§9.2).
* ADR-0147's four independence facts are unchanged, and readiness is still counted per juror operator.
* Genesis classes have no outsider and no jury, so neither touches the floor, 8k and 2M rows at launch.

**SW-6 (what does not become weighted).** Owner B. Tests T86, T70.
* **The S3 sampler sites** (`sample_draw(anchor, claim_id, seat_index, counter)`, `palw_layer_sample_v3.rs`) are a role
  inside a drawn panel, not a draw of operators. Unchanged.
* **The S1 assignment** (`palw_segment_assignment_v2`) is unchanged. It takes the full seat as `draw % seat_count` and the
  rotation as `(draw >> 32) % k`, from `H(assignment domain ‖ anchor ‖ claim ‖ seat_count)`. It depends on the seat's
  position, not on its weight, and it is independent of the lottery keys. So the full seat and a given segment's holder
  form a uniformly random ordered pair of positions, which is what makes §4.3's `P2 = E[A(A − 1)] / 20` exact under any
  draw.
* A weight-ordered full seat (for example, the heaviest panel member) was considered and rejected. It would send every
  full replay to the heaviest operators (capacity), tie the role to the attacker's split (one heavy Sybil takes the full
  seat), and change ADR-0133's rule under a different fence family.
* The admission jury (SW-5).

**SW-7 (Sybil resistance).** Owner B. Tests T87, T06.
* **The first seat is split-neutral.** Split one operator of weight W into two halves: its two keys are `2E_1/W` and
  `2E_2/W`, and the smaller has the distribution of `E/W`, because the minimum of two exponentials of rate W/2 is an
  exponential of rate W. So the chance that an operator, or the pieces it splits into, holds at least one seat does not
  change with splitting. Above the cap, staying whole only loses weight (SW-2).
* **Splitting yields only extra seats, and §4.3 prices the best split.** One operator holds at most one seat, and the
  undetected lie needs two (the full seat and the lied segment's holder), so an attacker must split. For a fixed stake,
  the finest split the 130,000 MSK floor allows maximises P2. At 5.20M: 0.1875 with 40 operators, against 0.1815 with 20
  and 0.0613 with 2. Uneven splits do worse. At 10.01M: 0.3440 with 77 equal operators, against 0.3315 with 60 floor
  operators plus one heavy (Monte Carlo of the normative key race). §4.3's table uses the finest split, so an attacker
  pays for its odds in stake however it splits, and no split beats the table.
* **Identity is priced, not verified (B-4 as corrected by SW-A3).** On t12 an operator id is one bond and possession of
  its key is proven, but a fresh key is free; SW prices seats in capital, as B-4 says every per-bond rule here does.

**SW-8 (one state, bind only in the anchor block, and the grinding surface; SW-A2, R2).** Owner B; A reviews the
acceptance and fold half. Tests T93, T89.
* **The seed** is the anchor block and the claim id, as today. The anchor block's producer can re-roll it at the cost of
  one block's proof of work per try; the claim id is fixed before the anchor exists. Both are unchanged from the uniform
  draw, and the anchor re-roll is the one post-seed lever this rule leaves.
* **The panel was already fixed at the anchor block** (derived there, on the parent state). The draft's post-seed
  levers were three: the anchor re-roll; a failed draw retried on a later state; and a derived binding dropped at
  acceptance (`PanelMismatch`, because acceptance reads the pre-object base) and re-derived later. Past
  `palw_rcore_plus` the second and third are closed by two rules:
  * **One state.** The derivation (the assembler's `PanelBound`), acceptance (`validate_panel_bound_v2_with_policy`)
    and the fold read the same state: the **pre-object base acceptance reads** (`palw_v2_pre_object_base_v1`: the
    parent state after this block's step 2), **advanced by the same block's earlier bindings in claim-id order**, with
    `stake` resolved at the anchor DAA. The eligibility filters (headroom, Valid lock, `Active`) and the weights are
    read there, never at the tip; a reorg across the bind re-derives on the new chain's state, as today's draw inputs
    are (T89). This is what post-edit 6's "read at the anchor state" asked for, and it is kept.
  * **Bind only in the anchor block.** A panel binds only in its own anchor block (a redraw's panel in the redraw's
    anchor block). A `PanelBound` carried later is refused, and a claim whose panel was not bound there voids
    `BindTimeout` (S0: no forfeit; a free-prompt claim keeps its abandon hold, SR-1). No later state exists on which a
    failed or dropped draw could be retried.
* **The weights cannot move after the seed.** The posted collateral of a bond registered before the anchor never rises
  (the code has no in-place deposit; U2's "top up" is a re-registration until §9.3 Q11 decides otherwise, B-4); it
  falls only by a slash, which needs a conviction. And the one state is fixed before anyone sees which seats won, so
  even an in-place deposit, if Q11 adds one, would move only panels anchored after it.
* **What this closes: the retry path (the relabelled 8.97M).** The draft named a "drop-grinding" surface: an attacker
  that carries the bind, or the block between the anchor and the bind, retires one of its own seated Sybils after
  seeing the anchor, and the panel is re-derived on a later state with the entries behind it moved up. Its upper bound
  (the attacker always gets that block and may retire any subset of its own entries) raised the design-point success
  from 0.50 to 0.73 at the threshold stake under both draws, which put the stake-weighted threshold at about 8.97M MSK
  (69 operators), against about 11 operators (1.43M) under the uniform draw (Monte Carlo). That surface **is** the retry
  path, which bind-only-in-the-anchor-block closes; §4.3's row is relabelled, and the v3.1 draft's §9.2 item "a panel
  fixed at the anchor" is removed (this is it).
* **A seed-time lever that stays (named):** the anchor block's producer re-rolls the anchor at one block's proof of work
  per try, pre-existing and the same under the uniform draw.

**SW-8 as integrated: the anchor block is an attempt block (IA-1a, IA-1c; `f422df49`).**
* **The rule.** Past `palw_rcore_plus` a panel may anchor only on an **attempt** block (algo 6 or 9,
  `is_palw_attempt_algo_id`), read by one predicate, `palw_block_may_anchor_a_panel_v1`, for the anchor walk and the
  fold alike. A claim's anchor block is the first attempt block of its chain at or past its slot `bind_base_daa() +
  anchor_delay`. It binds there, on the one state above, or it voids there `BindTimeout` (step 4c, S0; DL-1). Below
  the fence every lane but the chain-positionless ones may anchor, byte for byte as before; that set includes the
  heartbeat lane. The fence is resolved at the slot, and `palw_rcore_plus` is genesis-only, so the slot, the anchor
  and the block give one answer.
* **Why.** Under the stake draw the anchor re-roll is the one post-seed lever left, and this ADR prices it at one
  block's work per try. A heartbeat header's target is the network constant 2⁻²⁴: a re-roll there cost 2^24 hashes,
  "a couple of seconds of one CPU per try" (the resolver's doc). An attempt's execution commits to its header's pre-PoW hash (`execution_anchor_v3`), so
  every other attempt header costs one more inference. So the lever above now costs **one inference per try**.
* **The liveness residual (named).** A claim waits for the next attempt block. If none reaches its slot before
  `bind_base + window_bind`, it voids `BindTimeout` at that backstop, S0: nothing forfeited, the producer's
  reservation released, an FP claim on its abandon hold. The lattice keeps a slot reachable while attempts flow:
  `anchor_delay + max_beacon_gap` = 20 + 400 = 420 < `window_bind` = 600 (`PalwConsensusParamsV2::validate`, per the
  resolver's doc). **A heartbeat-only stretch voids, without forfeit, every claim whose slot falls in it**, and a
  network with no attempts has no claims to bind. The regime has been seen: before this regenesis, public t12 ran
  1,230 of its first 1,231 blocks as heartbeats when no producer ran (a scan by the audit session's read-only observer,
  2026-09-23).
  That is not a rate estimate for the launch.
* **Considered, not taken at launch:** anchoring on a committed seed after K heartbeats, so a claim can still bind
  through a heartbeat-only stretch. It is recorded, not specified, and stays an option for after launch (§9.2).
* **Monitoring (O-13):** the count of `BindTimeout` voids by anchor block, and the attempt-block ratio. No source gives
  an attempt-block rate for public t12, so **the bind window is to be derived from the rate measured after launch**
  (the operator: measurements after launch). Until then it stays 600.
* Tests: `t12_stake_draw_integration`. The one resolver sets `stake` iff `palw_rcore_plus` is active at the anchor.
  Genesis binds under the stake draw. The heartbeat at the slot resolves `sw8_anchor_delay` to `None`, and the anchor
  block voids the claim it does not bind at its own DAA with no bond's collateral moved. The fence-off twin waits out
  the bind window. T89's build = accept = fold, including a redraw and a reorg across the bind.

**SW-9 (throughput, capacity and liveness when stake concentrates).** Owner B (the rule), A (review, since it touches
T-2). Tests T91, T92, T94, O-2.
* **Liveness changes in exactly two places.** The eligibility filters are unchanged and SW changes who sits; but a draw
  now also refuses when less than 875‰ of the base weight is eligible (SW-10), and a panel not bound in its anchor
  block voids its claim (SW-8; as integrated the anchor block is an attempt block, so a heartbeat-only stretch voids
  every claim whose slot falls in it, IA-1a). Both are halts without forfeit (S0). With every genesis seat eligible neither fires.
  An operator with most of the stake below the cap sits on nearly every panel, once.
* **Saturation costs safety, not only availability (SW-A1).** The draft said "concentration costs availability, not
  safety"; that is **withdrawn**. Eligibility still reads the one ledger, so working honest seats fill their 500‰
  ceilings and drop out while idle Sybils, which carry no locks, stay eligible. With no floor the threshold falls with
  every saturated genesis seat: 17.29M at 8 of 8 eligible, 12.74M at 6, 8.19M at 4, 0.52M at 1, and at 0 of 8 any five
  idle Sybils (0.65M) fill every panel (§4.3). The regime is reachable: at the floor lane's ~1 claim/DAA, every licence
  on the coverage door, a genesis seat's lock load fits its 469,532 MSK of headroom over the normal lock life (312,315
  MSK), but not when rows are re-keyed by the producer's own DA sessions until acceptance + 4,200 (DA-5, DA-8): 549,778
  MSK with the genesis seats alone, 493,938 beside 5 idle 130k Sybils; beside 20 idle Sybils it still fits (386,858)
  (`v31_review_numbers.py`, model). SW-10 turns the cliff into a halt.
* **Concentration costs availability.** If a heavy operator is offline, the share of panels with one dead seat is that
  operator's inclusion probability. One dead seat blocks the coverage door (every seat must sign `Valid`) and holds the
  escrow to Final (SR-1 condition 2), which lowers p and producer capacity (T-2(d)). V1 (three V2 `Valid`s among the
  other four) and the S2 upgrade by any seat's full replay (Q-5) still license. O-2 reports each operator's inclusion
  against its stake, and the dead-seat share.
* **Load follows stake.** With inclusion roughly proportional to weight, every operator's locks grow in proportion to its
  collateral. The committed fraction therefore evens out across operators, and the population reaches its 500‰ ceilings
  together, instead of the smallest seats filling first as under the uniform draw. T-2(e)'s capacity formula (the sum of
  the seats' free halves) becomes the actual capacity rather than an upper bound. (That is also why saturation, above,
  can arrive for all genesis seats at once.)
* **The rate room counts effective ready operators (a T-2(a) amendment).** Today
  `per_span_c = ready × reference_work_per_span × utilization`, with `ready` counted in **bonds**
  (`model_registry_ready_seats`, STATE:9551). Under a stake draw the replay load lands on the heaviest operators. With
  equal compute per operator, capacity is set by the largest inclusion probability `π_max`, and under successive
  sampling `π_max ≤ min(1, seat_count · W_max / ΣW)`. Past `palw_rcore_plus` the room's `ready` therefore becomes

  `ready_eff = min(ready operators, max(seat_count, ⌊ΣW_ready / W_max,ready⌋))`

  over the class's ready operators, with SW-2's **capped** weights. This is a lower bound on capacity, so the room errs
  toward refusing.
  * Examples (`v31_review_numbers.py`): 8 genesis seats give 8; 8 genesis seats and 40 × 130k give 13 (of 48 ready
    operators); the same plus one ready operator of 20M still give 13 (the cap counts it as 1,000,000; the draft,
    uncapped, gave 5); one 20M operator and the 8 genesis seats give 8 (uncapped 5).
  * **The lever the cap bounds (SW-A6).** Uncapped, one heavy ready operator cut a class's room to `seat_count` once
    `W > ΣW_rest / 5`: 2,542,501 MSK beside 8 genesis seats and 40 × 130k. Capped, `ready_eff ≥ min(ready operators,
    max(seat_count, ⌊ΣW_rest / 1,000,000⌋ + 1))`, so that example stays 13.
  * **The residual, named (§4.2 #17, T91, O-2).** A class held only by small operators still has the lever: 40 ready
    operators of 130k give `ready_eff` 40, and one more ready operator at the cap cuts it to 6 (an uncapped 2.6M operator
    would cut it to 5). The cost is throughput, not safety: the room refuses, it never over-admits.
  * Counting operators also removes today's over-count of an operator's several ready bonds (moot on t12, where an
    operator is one bond).
  * At genesis nothing moves: the floor is not gated by the room, 2M is capped by `max_inflight_claims` (C7, T-2(b)),
    and the 8k row reads `ready_eff` = 8, its ready count. **After post-edit 5 the 8k row is room-governed**, so the rule
    applies to it as soon as operators other than the genesis seats are ready, and to every non-C7 class registered
    later.
  * It feeds `panel_rate_v1`, `panel_room_v1` and op 186 alike (T91).
* **The per-bond share** of T-2(a) (`⌈c_class / 2⌉` unlicensed claims per bond) is unchanged.

**SW-10 (the eligible-stake floor; fail closed; SW-A1).** Owner B; A reviews the acceptance and fold half. Tests T94,
T06, T92.
* **The rule.** Past `palw_rcore_plus`, the **base weight** of a draw is the sum of SW-2's capped weights over every
  operator that could sit: Active, at the seat floor, registered before the anchor, capable of the class (P7's
  definition), whether or not the one ledger admits it. The **eligible weight** is the same sum over the operators that
  also pass the headroom and Valid-lock filters. The draw binds only if `eligible × 1000 ≥ base ×
  eligible_floor_permille` (`PALW_DRAW_ELIGIBLE_FLOOR_PERMILLE_V1` = 875); otherwise it refuses with
  `PalwPanelV2Error::InsufficientEligibleStake` (appended). The check runs on SW-8's one state, before the key race, so
  build, accept and fold agree. (Whether a bought class's outsider population is also checked on its own is M4's to
  pin in T94.)
* **The executor's weight on both sides (IA-1b; `f422df49`, the M4 review's finding 1).** The rule as integrated is
  `(eligible + X) × 1000 ≥ (base + X) × eligible_floor_permille`. `X` is the executor operator's SW-2 weight (capped at
  `weight_cap_msk`) where that operator could sit on the class but for being the executor, and 0 where it could not
  (below the panel floor, not registered by the maturity floor, incapable of the class)
  (`palw_panel_stake_executor_bonds_judging_v1`). The outsider's floor takes the same term, and the reported
  `InsufficientEligibleStake` weights include it. **Why:** the 875‰ table was priced on a population of eight with the
  executor outside it, but t12's producers are its eight genesis cards. A genesis card's claim draws from the other
  seven, so one saturated seat gave `6/7 = 857‰ < 875‰`, and the busiest producer, whose own claims fill its own
  headroom first, could halt every other card's binding. With `X` the floor is computed over the population the draw
  would have without the executor exclusion, the executor counted eligible: it cannot sit on its own panel, so its
  load says nothing about who does. A genesis executor is back at 8 (7/8 binds, 6/8 refuses).
* **What `X` gives an attacker.** It relaxes the floor exactly as the same capital posted as an idle Sybil would
  (`eligible` and `base` both rise by it) and buys no seat, so **in total attacker capital no §4.3 threshold moves**.
  Counted in Sybil stake, §4.3's unit, a floor-bound row falls by at most `X` ≤ 1,000,000 MSK. §4.3 carries the rows
  the code re-ran.
* **Fail closed: a halt, not a capture.** A refused draw binds nothing; since a panel binds only in its anchor block, the
  claim voids `BindTimeout` (S0; the free-prompt abandon hold applies). **As integrated (IA-1c)** the void lands in the
  anchor block itself (step 4c), so the producer's reservation, the class's in-flight count and its room demand come
  back at once; nothing is forfeited. Rows do not mature through the halt (V-8 names
  it). Honest producers lose time, not stake.
* **Why 875‰** (`v31_review_numbers.py`; floor class, P2 with filing; the worst state a floor admits, the attacker
  choosing which k genesis seats are saturated and keeping all its own operators idle):

  | floor (‰) | worst admitted threshold | an honest-only 8-seat population still binds with up to (seats saturated)¹ |
  |---|---|---|
  | 0 (no floor) | 0.52M (4 operators; k = 1) | 3 |
  | 500 | 3.77M (29; k = 2) | 3 |
  | 667 | 6.63M (51; k = 3) | 2 |
  | 750 | 8.19M (63; k = 4) | 2 |
  | 800 | 10.53M (81; k = 5) | 1 |
  | 850 | 11.31M (87; k = 5) | 1 |
  | **875** | **12.74M (98; k = 6)** | **1** |
  | 900 | 12.74M (98; k = 6) | 0 |
  | 950 | 15.08M (116; k = 7) | 0 |
  | 1000 | 17.29M (133; k = 8) | 0 |

  ¹ The column is the smaller of the stake floor's limit and the 5-operator minimum: with fewer than 5 eligible
  operators the draw refuses `InsufficientEligibleBonds` whatever the floor (T92), so an honest-only 8-seat population
  never binds with more than 3 saturated. `v31_review_numbers.py` counts the stake floor alone and prints 8 at 0‰ and 4
  at 500‰; from 667‰ up the floor is the tighter limit and the column is the script's.

  875‰ is the highest floor at which an honest-only genesis population still binds with one seat saturated (7 of 8 is
  exactly 875‰). The uniform draw today refuses only below 5 eligible operators (up to 3 of 8 saturated). §4.3 gives the
  thresholds for 7…0 of 8 eligible under the floor, per model.
* **Residual, named.** An attacker that fills its own bonds with locks until the eligible share falls under the floor
  can stop binding: a halt without forfeit, one of V-8's halts, not a capture.
* `stake: None` never refuses on stake (T88).

---
