> **Archived verbatim from ADR-0152 v3.1** (branch `docs/adr-0152-v31-postedits` at `9ed1adced`,
> lines 1–401 of `docs/adr/0152-account-stake-staged-reserve-and-vested-rewards.md`): the title, the status block, and the change tables (v3.1 changes, v3.1 post-edits, integration amendments IA-1…IA-15, v3 changes).
> Not normative. The rules are in [spec/palw](../../../../spec/palw/00-index.md); the decision record is
> [ADR-0152](../../../../adr/0152-account-stake-staged-reserve-and-vested-rewards.md); the reading guide is [README.md](README.md).

# ADR-0152 — A bond is standing stake, a claim reserves until its licence, and a reward vests until its conviction window closes (R-core+) — v3.1

* **Status.** v3.1, 2026-09-24, **with its post-edits applied** (the 13 items of `v3.1-postedits.md`, relayed by the
  audit session after the v3.1 workflow started, and the §3.14 body sync; table "v3.1 post-edits (2026-09-24)" below),
  **and with its integration amendments applied** (IA-1…IA-15, table "Integration amendments (2026-09-24)" below: the
  text brought in line with what the integration line implements and with the operator's decisions of the same day).
  **ACCEPTED for testnet-12 by the operator. Partly implemented:** the integration line `rcore/int-2` (`8e1ce34a`)
  carries the R-core+ body merged so far (S-1…S-4, M2 Phases 1–2, M3, S-6/S-7, F4 part 1, the stake draw, the vesting
  rows, SEAT-R/SEAT-S, P6). It does **not** yet carry the parts listed in that table's last part, "What the integration
  line does not have yet": the class-derived deadlines and the 2M closure, A-held and the shard court, M2 Phase 3,
  S-5 (SR-9, object 56) and S-8, F4 part 2, Phase 2's node stages (the seat DA answer and the producer's V2 DA responder
  among them), the `AnyValid` fence's H-2, and the licence-time `G_res` field. The v3.1 text said "Not
  implemented" (the P7 type, `PalwPanelDrawPolicyV1::stake`, was then committed with `None` everywhere at `f1dfb33b` on
  `feat/t12-rcore-stake-draw`); the stake draw is now integrated (`f422df49`, IA-1). v3 is kept verbatim as
  `0152-v3-snapshot.md`. v3.1 makes four changes, plus the corrections they force (table "v3.1 changes" below, N1–N13):
  1. **F1/F2 take the audit session's names, types and discriminants** (`f2f1_spec.md` §3.2, §4.1, §4.4, which is
     authoritative): the fence `palw_offence_attribution`, `PalwOffenceKindV1::PanelFalseValidV2 = 3` and
     `ExecutorRefuted = 4`, contradictions 9–12, `PalwClaimStateV2.job_identity`, and the four liability fields. v3's own
     spellings (`job_anchor` as a stored field, the `claim_rcore` side record, `ExecutorRefuted = 3`, `JobMismatch = 9`,
     `DaDefault = 4`, `CourtConviction = 5`) are gone; R-core's two consumed-offence kinds are now `DaDefault = 5` and
     `CourtConviction = 6`. There is one v22 layout (§6).
  2. **The panel draw is stake-weighted before launch** (operator decision Q4, 2026-09-24), reversing ADR-0124 D5 /
     ADR-0130's one-ticket-per-operator lottery on t12 (§3.14). §4.3's collusion table is re-derived under it
     (`v3calc/v31_stake_draw.py`).
  3. **F1-M and F1c are inside the regenesis gate** (operator decisions): model-class fraud (8k/2M relabel, forged
     output on tiled/A16 decode) and garbage logits on an honest step tree are attributable at launch; model-class
     production starts at genesis.
  4. **§9 Q1–Q8 are decided** (Q4 as above; the others take v3's defaults) and leave the open list.
  * **The v3.1 draft was reviewed** (lenses stake-security SW-A1…SW-A6 and spec-impl R1…R12; draft kept as
    `0152-v3.1-draft-snapshot.md`). This final v3.1 applies every finding it agrees with; the third table in "v3.1
    changes" maps each one, and "v3.1 review disposition" at the end gives the partial adoptions with reasons. The
    review changed the draw in five ways: the panel is fixed in its anchor block, on the state acceptance reads (SW-8);
    an eligible-stake floor makes a saturated honest population halt instead of seating idle Sybils (SW-10); a weight
    cap of 1,000,000 MSK per operator (SW-2); operator ids are unique on t12, so an operator is one bond (SW-2, §6); and
    ADR-0147's admission jury is **not** weighted (SW-5). The v3.1 tables absorbed the review first; the post-edits
    carry it into the body (§3.14, §4.3, §6, §8.1, §9).
  * **The post-edits (2026-09-24)** change, beyond the draw: the held set (only C7, the 2M row, is held to Final; 8k is
    released at licence again, correcting V3S-09; T-2(b)); the escrow hold (C7 only, U1); a producer floor gate (U2,
    amending B-4) and a post-Final FP producer tier (U3); contradiction `PromptNotAnchored = 13`; F2's finding and
    `ClaimUnderSession` (J-3); SEAT-R as a ship rule with F2's fence (Q-7); SR-10 moving to M4; the S spec's v22 table
    (§6); the shard-court and admission rules under the fence (J-5, J-8); and one open launch gate, the `AttnFused`
    held-class gap (§4.2 #18, §8.3). Item 6 of the post-edits (a summed free-stake weight map) is **superseded** by the
    review disposition and the P7 type, and is not applied.
  * v1 was reviewed adversarially (verdict NEEDS CHANGES, X1–X30). v2 applied those.
  * v2 was reviewed adversarially a second time (`adr0152v2_review_synthesis.md`, verdict NEEDS CHANGES,
    findings F1–F21, four of them critical). v3 applies the operator's seven decisions of 2026-09-24
    (§1.5, "v3 decisions") and the review's required changes, plus the findings of the Phase 2 plan
    (`phase2-plan.md` §0, §5.3).
  * The v3 draft was checked by three verifiers (lenses: decisions C1–C13, security V3S-01…13,
    implementability IMPL-1…18). This final revision applies every finding it agrees with; the rest are
    answered in "v3 review disposition" at the end. The second table in "v3 changes" maps each applied finding.
  * The work runs in a **mandated order**: F2 → F1 (with F1-M and F1c) → F3 → F4 and the stake-weighted draw →
    fold/reorg/restart tests → only then X10 (§7). Ownership is in §7.
  * The regenesis takes this ADR whole or not at all. The gate is §8.3: F1 (including F1-M and F1c), F2, F3, F4 and
    the stake-weighted draw GREEN, the fold/reorg/restart tests GREEN, and a short drill. **The long soak (v2's T10) is
    no longer a pre-launch gate.** It becomes the post-launch observation program on public t12 (§8.4).
* **Date.** 2026-09-24 (v1, v2, v3, v3.1).
* **Scope.**
  * testnet-12 only, behind two genesis-armed fences: the audit session's `palw_offence_attribution` (F2/F1, spec §3.1)
    and this ADR's `palw_rcore_plus` (everything else, including the stake-weighted draw), which requires the first
    (§6). X10 has its own later fence, `palw_rcore_attributed_charging`, which is not declared at the regenesis: the
    field is added and armed with M6 (§3.2 SR-5; S-SPEC §1e, post-edit 9).
  * Isolation is at the params-fingerprint level. Every v21 build since the audit fence already cannot run
    testnet-11 (F20). v22 keeps that, and this binary refuses a testnet-11 datadir at startup (§6).
* **Amends, on testnet-12:**
  * ADR-0151's option-A addendum: the reservation is staged (§3.2).
  * ADR-0042 D10 and ADR-0124 D1/D2: `Final` writes a vesting row, not a payout (§3.3).
  * ADR-0124 D3: the duty's `3 × reserved` term is retired (§3.5).
  * ADR-0062 SA-1…SA-7 (the DA court): sessions are keyed by `(claim, accuser)`, answer drawn units, run after
    licence and after Final, and give the paused time back (§3.11, F3).
  * ADR-0133 S2/S3: a sampled seat never counts toward a quorum. S2 never carries a claim to `Final` (§3.12, F4).
  * ADR-0137 D5 as amended by the panel-room fix `e93be0f2` and its review `f8c91f19`: the rate room stands, and the
    hold to Final with its static cap is keyed on C7 (the 2M row), not on every held-context class (post-edit 5, the
    code fix on `fix/t12-panel-room`), with one t12 addition (T-2(c)).
  * ADR-0111 D3 (one `StepLeaf` demand per seat per claim, inside the seat's draw sample): past this fence the
    named leaf of a DA session is free and the demand is keyed by the session (DA-3).
  * The DoS audit's #12(c): the registration floor goes back to the producer floor (B-2), reversing
    `139c9215`'s panel-floor rule, which the audit session agreed to.
* **Keeps:**
  * ADR-0151 D1–D5, ADR-0130 λ, ADR-0091 buyback at Final, ADR-0144 §9, Mainnet Decision A (`1827850e`).
  * The DoS branch's two-clock rework (`6bb8c844`, `a8c19379`), its per-door lock pricing (`8db098d0`, now
    recounted per set, §3.12), and its #8 reversal, narrowed (X1, X6).
  * **The DoS audit's #10 (the second ReceiptTimeout forfeit)**, which v2 had removed. It stays until X10 arms
    (operator decision 1).
* **Line references.**
  * `HEAD` for v3 is **`f8c91f19`**, the tip of `fix/t12-panel-room`: merge `4064364e`, the panel-room fix
    `e93be0f2`, and that fix's review `f8c91f19` (a held class stays held to Final and to its static cap, which the
    room re-review's N1 found also held the 8k row and post-edit 5 re-keys on C7; the own attempt keeps the room step 3
    found; the room is exact). The draft was anchored on `e93be0f2`; line numbers
    below that were taken at `e93be0f2` are marked so, and every reference should be re-anchored **by symbol**.
  * **v3.1 re-anchors on `0533e1de` (R11)**, the branch tip two commits past `f8c91f19` (`92a9658d` adopts the room
    review's probes; `0533e1de` makes the class gate take `PalwGatedClaimV1` and pool a free-prompt commitment before
    counting it, T-2(a)). S takes `0533e1de`. `palw_panel_v2.rs` and `processor.rs` are byte-identical between
    `f8c91f19` and `0533e1de`; STATE anchors cited for T-2 and SW-9 are at `0533e1de` and marked so.
  * **The post-edits anchor §3.14 on `f1dfb33b`** (R5): the launch candidate `c8652a97` (`0533e1de` + the licence-stall
    fix + the 100M genesis row) plus P7, the `stake` field. The licence-stall fix moved `palw_panel_v2.rs` by 13 lines
    and P7 by 41 more above the draw, so §3.14 gives each DRAW anchor at `f1dfb33b` with its `0533e1de` line in
    brackets; the STATE anchors it cites (`panel_room_v1`, `panel_rate_v1`, `model_registry_ready_seats`, the jury call,
    `DuplicateOperator`) are the same lines at `0533e1de`, `c8652a97` and `f1dfb33b`.
    This revision re-read at `f8c91f19` the code behind its load-bearing new claims (the FP abandon hold, the
    supplementary door, the anchor tick in `license_claim`, the verdict-to-answer map, the held-class hold, shard
    dormancy, the prefill draw, the per-operator lottery, the header fallback, the leaf-demand rule, the Flat checker,
    the genesis payout keys, the consumed-offence writers, the context-set test and gate). Other line anchors taken
    from the verifiers' evidence are **UNVERIFIED** as line numbers until re-anchored by symbol.
    Paths are under `consensus/core/src/` unless stated. `STATE` = `palw_state_v2.rs`, `OFF` = `palw_offence_v1.rs`,
    `PROC` = `consensus/src/pipeline/virtual_processor/processor.rs`, `PANEL` = `kaspad/src/palw_panel.rs`,
    `DRAW` = `palw_panel_v2.rs`, `SPEC` = the audit session's `f2f1_spec.md` (read at `wt-trial` 4064364e; its line
    references are its own), `ADDENDUM` = its `f1c_f1m_spec.md` §4-bis, `S-SPEC` = its `s_spec.md` (the S half, with
    §(10)'s user decisions U1–U3). v3.1 re-read the draw at `f8c91f19` (`derive_panel_v2_with_policy`,
    `PalwPanelDrawPolicyV1`, `palw_panel_operator_entries_under_v1`, `palw_panel_outsider_seat_v1`,
    `palw_admission_jury_v1` and its fold caller `admission_jury_seated`, `PalwPanelValidLockV1`,
    `palw_segment_assignment_v2`, `panel_rate_v1` / `model_registry_ready_seats`) and the genesis at `5bf72b46`
    (`a27f8f44`: the 858M community table, now with a second 100M row; the eight seats unchanged).
  * **The integration amendments anchor on the integration line** (`rcore/int-2` in its worktree `wt-int2`) at
    **`8e1ce34a`**. That is `68f0d672`, the merge of `feat/t12-s4` `c3fe99cd` (S-4's conviction funnel, over the M3/S
    re-review fixes `479cdfa3`), plus `8e1ce34a`'s test. At `68f0d672` `PALW_RCORE_VESTING_ROWS_LANDED_V1` is `true`
    and `claim_g_v1` reads the liability row first (IA-7, IA-8). `8e1ce34a` adds the test that on the floor and the 8k
    row, under the armed residual price, a licence needs no top-up (L-4). The amendments cite code **by symbol
    only**; no line number is given for the integration line.
  * v2's references (at `50565f55` / `5755a839`) are kept where v2's text is kept. Re-anchor them by symbol.
  * The review's probes are in the audit session's `wt-audit/consensus/core/tests/adr0152*_*.rs`.
* **Where the figures come from.** Constants and fold tests; the review's scripts `adr0152v2_q.py` and
  `adr0152v2_q2.py`; v3's scripts `scratchpad/adr-rcore/v3calc/v3_numbers.py` and (this revision)
  `v3calc/v3_numbers_r2.py`, which use the same inputs (E = 444,562,014,000 × 720‰; `w`, `R` from §2) and reproduce
  the review's lock values and the verifiers' `v3review/ev_v1door.py` and `v3review/extra.py` outputs exactly (all
  three re-run for this revision). v3.1 adds `v3calc/v31_stake_draw.py` (output beside it in `v31_stake_draw.out`),
  which reproduces v3's uniform thresholds (20 / 16 / 10 / 8 / 56 operators) with the same code before it computes the
  stake-weighted ones, and, for the review, `v3calc/v31_review_numbers.py` (`v31_review_numbers.out`: the saturation
  cliff, SW-10's floor, the honest weight vector, the room lever, the jury). The reviewer's independent scripts
  (`scratchpad/review-sw/{base,cliff,jury,indep,grind}.py`) reproduce every figure they share with these.
  **None was measured on a live t12 claim.** A figure from a model rather than from
  code is marked **(model)**. A fact v3 did not check against code is marked **UNVERIFIED**.

---

## v3.1 changes

Owner **A** = the audit session, **B** = the ADR owner. Phase codes as in "v3 changes" below; v3.1 adds the stake-weighted
draw to M4.

| item | what changed | where | owner | tests | phase |
|---|---|---|---|---|---|
| **N1** | F2's fence is the audit's `palw_offence_attribution` (Some-only, t12-only, DAA 0; requires `palw_objective_offence`, `palw_audit_2026_09_23`, `palw_verification_v2`, `palw_economic_safety`; F1 adds `palw_prefill_draw`). `palw_rcore_plus` requires it at or below its own height | §6 fences | A (M1) | T24, `palw_offence_attribution_is_t12_only` | M1 |
| **N2** | `PanelFalseValid` past the fence is `PalwOffenceKindV1::PanelFalseValidV2 = 3` with `PalwPanelFalseValidEvidenceV2 { version 2, claim_id, accused_seat, receipt: PalwFalseValidReceiptV1 { Full(PalwSeatReceiptV2) = 0, Segmented(PalwSeatReceiptV3) = 1 }, contradiction, reporter_reveal }`: no `network_domain`, no `executor_pubkey`; `reporter_reveal` empty on t12 and outside every conviction hash (its size bound is F7's to set; SPEC caps only the whole evidence, R10). One adjudicator, `palw_check_panel_false_valid_v2`, called by the processor (with the signature) and the fold (without); it binds by root and never compares `claim_id`. Kind 1 is refused past the fence (`SupersededOnThisNetwork`); `ExecutorEquivocation`, `CourtExecutorGuilty`, `ConflictingPermit` and `Legs` are refused by name as contradictions. Replaces v3's J-3 gate and the M4 wire item of Q-6 | J-3, Q-6, V-5, §6 | A (M1) | T46a–T46n (SPEC §3.6) | M1 |
| **N3** | The stored job identity is `PalwClaimStateV2.job_identity: Hash64`, appended after `rights_reserved` (attempt: the carrying header's `execution_anchor_v3`; FP: `palw_fp_job_pin_v1(commitment)`; 0 = not recorded, never convicts, never refuses admission). `PalwPanelLiabilityRecordV1` appends `job_identity`, `free_prompt`, `trace_root`, `segment_count` after `settled_at_final`. Written only when `offence_attribution_active`. v3's `claim_rcore` side record is dropped: R-core's per-claim fields are appended to `PalwClaimStateV2` after `job_identity` | J-1, §6 rows 1–8 | A (M2; S for R-core's fields) | T-THREAD, T18g, `f1_job_identity_survives_reorg_across_admission` | M2; S |
| **N4** | Contradictions `IdentityMismatch = 9`, `OutputMismatch = 10`, `ForgedOutputTiled = 11` (F1-M), `LogitsNotStepOutput = 12` (F1c); identity checks J1–J5 (audit), including the full-context check | J-5 | A (M2) | T18b, T18d, T18e, T18f, T18h, T18m, T18c(iv) | M2 |
| **N5** | `PalwOffenceKindV1::ExecutorRefuted = 4`; the accused is the executor bond; evidence `{ version 1, claim_id, contradiction ∈ {5, 6, 8, 9, 10, 11, 12}, reporter_reveal }`; ledger key one per claim | J-4 | A (M2) | T18b, T18c | M2 |
| **N6** | R-core's consumed-offence kinds take the next free discriminants: `DaDefault = 5`, `CourtConviction = 6`. They are written only by the fold; a **filed** `ObjectiveOffence` of kind 5 or 6 is refused by name in the processor and the fold (R1). Keys: `palw_offence_id_v1(DaDefault, producer, H(PALW_DA_OFFENCE_KEY_DOMAIN_V1 ‖ claim_id))` and `palw_offence_id_v1(CourtConviction, producer, H(PALW_COURT_OFFENCE_KEY_DOMAIN_V1 ‖ claim_id))` | V-2b, DA-7, §6 row 19 | A (M3, S) | T81 | S; M3 |
| **N7** | Forfeiture follows SPEC §4.5: contradictions 5, 6, 8, 11, 12 (`StepArithmetic`, `StepStructural`, `ForgedOutput`, `ForgedOutputTiled`, `LogitsNotStepOutput`) record the root and forfeit by root; contradictions 9 and 10 (`IdentityMismatch`, `OutputMismatch`) record root 0 and forfeit by claim. "Kind" means `PalwOffenceKindV1` only (R7) | V-2b | A (M2) | T18k, T81 | M2 |
| **N8** | The vesting row **copies** `job_identity`, `free_prompt`, `trace_root`, `segment_count` (agreed with the audit); it does not reference the liability row | V-1, J-2 | A (S) | T62, T28 | S |
| **N9** | `ProducerWithholding` stays a kind-3 contradiction against `Valid` signers only when the withholding is DA-confirmed (a DA-7 default), never against `Sampled`, `Incapable` or `Unavailable` filers, and at the S4 tier (capped at `3 × G`, m = 3); DA-7's own S4 on covering signers uses the same (seat, claim) ledger key, so it is charged once (agreed) | §3.6 X7, DA-7 | A (M1, M3) | T32, T46b, T66 | M1; M3 |
| **N10** | The kaspad evidence filer is Phase 2 (agreed): P2-8c builds `PalwPanelFalseValidEvidenceV2` and P2-8 builds `ExecutorRefuted` | §7.3 | B (P2) | T54d | P2 |
| **N11** | F1-M and F1c are in the gate (operator decisions); model classes produce from genesis. The audit's "keep model classes Held with RT#2" (SPEC §5 item 2) is not taken | J-7, §8.3 | A (M2) | T18e (8k, 2M), T18m, T18c(iv) | M2 |
| **N12** | R-3's commitment binds the consumed evidence's `evidence_id`, not only the offence key: the audit's ledger keys are one per (seat, claim) and one per claim, so they are public at bind, and a speculator committing to every key would otherwise win every reward | R-3 | A (S) | T39, T75 | S |
| **N13** | A kind-3 or kind-4 conviction is refused `ClaimUnderSession` only for an open court past `palw_rcore_plus`; an open R-core DA session does not block it, and the conviction closes every open session on the claim (exposure returned, `refuted_held` refunded) | J-3, DA-6 | A (M3) | T46n, T69 | M3 |
| **SW** | **The stake-weighted panel draw** (Q4): weight = the posted collateral of the operator's one bond (operator ids are unique on t12), in whole MSK, capped at `PALW_DRAW_WEIGHT_CAP_MSK_V1` = 1,000,000; key `L_i / W_i` from an integer `−log2`, the smallest keys sit (successive sampling without replacement, per operator); the ADR-0147 outsider uses it, **the admission jury does not**; S3 site assignment and ADR-0133's full-seat assignment do not change; the panel is drawn only in its anchor block, on the state acceptance validates it against (SW-8); the draw refuses when less than 875‰ of the base weight is eligible (SW-10, fail closed); the rate room counts effective ready operators over capped weights. Undetected-pair collusion turns EV-positive at **17.29M MSK** at the design point (133 × 130k operators beside the eight genesis seats) and at **12.74M** in the worst saturation state SW-10 admits, against v3's 2.60M | §3.14, §4.3, T-2(a) | **B (M4)** | T06, T85–T94 | M4 |
| **Q1–Q8** | Decided: Q1 no RT#2 cap; Q2 C7 by the ≥ 1,000-span rule; Q3 `Sampled` paid as `Valid`; Q4 stake-weighted draw; Q5 quantum maturity unchanged (residual named); Q6 FP licences do not tick the clock; Q7 no `BondRegistered` per-block cap; Q8 DA bytes and draw grinding accepted | §9 | — | — | — |
| **Tests** | Phase 2's T46 (queue lemma) is renumbered **T58** so that the audit's T46a–T46n keep their file (`t46_false_valid_real_claim.rs`); §7.3 lists the rename among the changes to `phase2-plan.md` (R12). v3's T60, T61 and T63 are retired into the audit's T46a–n, T18* and T-THREAD; T61c's premise was false (FP is a fixed point too); T31 becomes a refusal test (kind 1 superseded) | §8.1 | A, B | — | — |

**The review of the v3.1 draft, as applied** (two lenses: stake-security SW-A1…SW-A6, spec-impl R1…R12). Partial
adoptions are explained in "v3.1 review disposition" at the end.

| id | what it found | closed by | owner | tests | phase |
|---|---|---|---|---|---|
| SW-A1 | Eligibility still reads the one ledger, so working honest seats fill and drop out while idle Sybils stay eligible; with no stake-quorum floor the threshold falls to 0.52M at 1 of 8 genesis seats and to any 5 Sybils at 0 of 8 | SW-10: the draw binds only if ≥ 875‰ of the base weight is eligible, else `InsufficientEligibleStake` (the claim does not bind; a halt, not a capture); the regime is reachable (§4.3, `v31_review_numbers.py`); SW-9's "availability, not safety" corrected; §4.3 rows for 7…0 of 8 | B | T94, T06 | M4 |
| SW-A2, R2 | The panel is already fixed at the anchor block (derived there, on the parent state); the post-seed levers are the anchor re-roll, a failed draw retried on a later state, and a derived binding dropped at acceptance (`PanelMismatch`, because acceptance reads the pre-object base) and re-derived later | SW-8 rewritten: one state (derivation reads the pre-object base acceptance reads, advanced by the block's earlier bindings in claim-id order) and **bind only in the anchor block** (a claim not bound there voids `BindTimeout`, S0); the 8.97M row relabelled as the retry path that rule closes; SW-2's first reason withdrawn; the §9.2 "panel fixed at the anchor" item removed | B | T93, T89 | M4 |
| SW-A3, R3 | `palw_operator_id_unique` (armed at t12 genesis) makes one operator exactly one bond, and the draw's safety needs it; it was not a prerequisite | §6 prerequisite with a T24 negative case; SW-2 weight = the one bond's posted collateral, not moved by the one ledger (which decides eligibility only); T87 and T93 rewritten; B-4 and §4.1 (viii) reworded (possession is proven on t12; a key is still free) | A (§6), B | T24, T87, T93 | S; M4 |
| SW-A4 | Weighting the ADR-0147 jury refuses honest classes the genesis seats do not load (20 × 130k ready: 0.1325 per audit against 0.8769); it was outside Q4 | **The jury stays ADR-0147's** (not weighted); the trade-off and the kept residual are stated (SW-5); the weighted jury is deferred (§9.2) | B | T90, T88 | M4 |
| SW-A5 | The threshold depends on the honest weight vector, not its sum (one 7.51M operator gives 0.52M; 100M as one operator 73.19M, as 769 × 130k 259.61M) | §4.3 residual and §9.3 Q10 restated on the weight vector; the cap (SW-2) makes operators above 1,000,000 MSK gain nothing by staying whole; O-2 reports the DP threshold of the live eligible weight vector | B | T06, O-2 | M4; O |
| SW-A6 | `ready_eff` lets one heavy ready operator cut a class's room to `seat_count` (W > ΣW_rest / 5: 2,542,501 MSK for 8 genesis + 40 × 130k) | Bounded by the cap (that example stays 13); the residual (a class held only by small operators: 40 → 6 for one 1,000,000 MSK operator) is named in SW-9, §4.2 #17, T91, O-2 | B | T91 | M4 |
| R1 | Nothing refused a **filed** `ObjectiveOffence` of kind 5 or 6, and the exhaustive `match kind` sites had no arms | V-2b: refused by name in `palw_ledger_evidence_id_v1` (OFF:174), `palw_verify_objective_offence_v1` (OFF:381) and the fold's `match kind` (as `CourtExecutorGuilty` is at OFF:388 and STATE:11301); the two consumed-offence keys given | A | T81 | S |
| R4 | F1-M and F1c had no defined function to test | J-5 cites the audit's addendum (`f1c_f1m_spec.md` §4-bis): the core attempt rule `CoreV1`, its chain inputs, the fixture families, and the F1c hash form, whose spike is done; a head that fails the predicate is refused at admission | A | T18e, T18m, T18c(iv) | M2 |
| R5 | §3.14 had no code anchors, no type, and an ownership conflict | anchors in §3.14; `PalwPanelStakeDrawV1` defined; the one resolver `palw_panel_draw_policy_at` sets it; T89 restated; §7.1/§7.2 give S the struct field and M4 a PROC slot after M3 | A (S), B (M4) | T88, T89 | S; M4 |
| R6 | SPEC T18b, T18c(ii) and T46n exercise the v1 DA paths that M3 turns off | J-7: they run on a harness with `palw_rcore_plus` forced off (fence-off twins); M3 adds named R-core twins (T18b-R, T18c(ii)-R, T46n-R) | A | T18b, T18c(ii), T46n | M2; M3 |
| R7 | "Kinds 5, 6" meant contradictions and collided with `PalwOffenceKindV1` 5 and 6 | N7, V-2b, T81 say "contradictions" | A | T81 | — |
| R8 | The liability record's `licence_door` must be optional (rows are written for claims voided before any licence) | §6 row 8: `Option<PalwLicenceDoorTagV1>`, `basis_k` 0 when none | A | T28, T41 | S |
| R9 | J-2's target lacked `job_identity` and `trace_root` | J-2 lists both, with their resolution order | A | T62 | M2 |
| R10 | "≤ 256 B" for `reporter_reveal` is not in SPEC | dropped; the slot is empty on t12 and its bound is F7's | A | — | — |
| R11 | The branch tip is `0533e1de`, which moved the room gate | header re-anchor; T-2(a) restated (pooled free-prompt commitment); SW-9 cites STATE:9523 / :9551 at `0533e1de` | A (S), B | T21, T91 | S |
| R12 | `phase2-plan.md` still calls the queue lemma T46 | §7.3 lists the rename | B | T58 | P2 |

---

## v3.1 post-edits (2026-09-24)

Thirteen items arrived after the v3.1 workflow started (`v3.1-postedits.md`; items 4 and 12 are user decisions relayed
by the audit session). Each is applied **in place** in the sections named below, not only here. The last two rows are
the body sync the v3.1 tables promised but the body had not absorbed, and the corrections from the review of this
revision. Owners and phases as in "v3 changes"; **P6** and
**SEAT-R** are B's named items outside the M-numbering.

| item | what changed | where | owner | tests | phase |
|---|---|---|---|---|---|
| **PE-1** | `PalwPanelContradictionV1::PromptNotAnchored { binding, proof: PalwPromptProofV1 { Tile(PalwPromptIdsOpeningV1) = 0, Whole = 1 } } = 13`, appended; attempt lane only; for prompts over 4,096 ids (`PALW_J5_INLINE_PROMPT_IDS_V1`: the 2M row). A `Whole` recompute is charged against `PALW_HEAVY_PROMPT_IDS_PER_BLOCK_V1` = 2^18 ids **before** it is computed (a processor counter, mirrored in the fold), so at most one metered `Whole` per block; forfeiture by claim; kind 4 admits it (ADDENDUM §4-bis.3) | J-4, J-5, J-6, V-2b, V-5, R-3, Q-6, §4.1 (ii), §6 rows 20 and 30, §7.3 P2-8 | A (M2) | T18e (2M: 13 `Tile` and `Whole`), T18u | M2 |
| **PE-2** | F2's finding: `PalwFalseValidFindingV1.execution_proving` becomes `acts_on_claim: bool` plus `forfeit: PalwForfeitScopeV1 { None, ByClaim, ByRoot }` (9, 10, 13 claim-proving and by claim; 5, 6, 8, 11, 12 execution-proving and by root). `ClaimUnderSession` refuses kinds 3 and 4 **only on an open court**, never on a DA session (held-DA or R-core), below and above `palw_rcore_plus` (ADDENDUM §4-bis.7) | J-3, J-7, §3.6 S2, N13 | A (M1's amendment, landing with M2) | T46n (restated), T18y | M1; M2 |
| **PE-3** | **SEAT-R is B's (F4) and a hard ship rule: it ships in the same binary as F2's fence**, or F2 slashes honest seats through the material arms and through kind 4's `CourtFraud` cascade at `Whole`. A full-mask `Valid` comes only from the attempt or free-prompt replay; a replay mismatch is terminal; the material, interval and capture-sample arms are evidence-only; partial seats sign only from the S1 resume; SEAT-S3 (the addendum's S-3): a pooled capture is used only after `verify_material(full roots + anchor) == Matches` (ADDENDUM §4-bis.10 has the line references). The addendum's other seat fixes, SEAT-S1, SEAT-S2 and SEAT-S4, and the drill hook `PalwDrillFaultV1` are A's (§7.1 "Seat fixes"); this ADR prefixes all four with SEAT- because S-0…S-8 are S-SPEC's commits | Q-7, §7.1 (M1, M4, Seat fixes), §7.2, §8.3 item 6 | B (SEAT-R with SEAT-S3); A (SEAT-S1, SEAT-S2, SEAT-S4, `PalwDrillFaultV1`) | T18p-M, T54e | M4, shipped with M1's fence |
| **PE-4** | User decisions: SEAT-0 is patched **live** after a drill (fp stays `c746f07c`; one host at a time; the user confirms the window first) and also goes into the launch line; Kimi-kernel classes are refused at admission under `palw_offence_attribution`; the FP `output_root` rendered rule is unified, so `OutputMismatch` (10) covers FP model claims (FP roots and `misaka-palw-derive` move once, at the regenesis); the canonical job is fixed to `(n_ctx/8 − 1, 2)` for model classes under the fence; F1-M (a) and F1c are inside the gate (as v3.1 already said) | §1.5, J-5 (admission, 10), §6 genesis, §7.1 M0, §8.3 | A | T18w, T18s (with an FP model claim), SEAT-0's T1–T5 regressions | M0; M2 |
| **PE-5** | **The held set, correcting V3S-09.** 8k is released at licence; only C7 (2M) is held to Final with its static cap. C7 = `verification_window_spans ≥ PALW_RCORE_C7_WINDOW_SPANS_V1` (1,000), united with `palw_rcore_conservative_classes` once R-core+ lands. This restores the user-approved v2 T-2(a)/(b); the code fix is on `fix/t12-panel-room` (workflow wxt13xc23; WIP `wip/t12-c7-held-restore-20260924`). Every "8k is also held" statement is removed | header, §2, SR-5, T-2(a)(b)(c), §3.9, SW-9, §6, T20, T21, T91, O-2, 運用者向け要約 | B (the launch-line room fix: finishing `wip/t12-c7-held-restore-20260924`, 5bcc52d7, and fast-forwarding `fix/t12-panel-room`, HANDOFF §2/§4 step 2; A reviews and runs the battery); A (S: the T-2 rules) | T20, T21 (incl. `panel_room_short_class_is_released_at_licence`), T91 | launch line; S |
| **PE-6** | **Superseded, not applied as written.** It asked for `palw_bond_free_slashable_v1(state, params, bond, now_daa, raw_depth)` summed per `operator_id` as the weight, typed `PalwPanelDrawPolicyV1::stake: Option<PalwPanelStakeWeightsV1>` (a `BTreeMap`). The review disposition (SW-A3, SW-A5) and the committed P7 type decide otherwise: free stake under the one ledger (`palw_bond_committed_v1`, behind `palw_bond_free_slashable_v1` and the headroom test) is the **eligibility filter**; the **weight** is the one bond's posted collateral in whole MSK capped at 1,000,000 (operator ids are unique on t12); the type is `PalwPanelStakeDrawV1 { weight_cap_msk, eligible_floor_permille }`, `Copy`, no map. Kept from item 6: the draw reads its inputs at the anchor, never the tip, under today's reorg discipline (SW-8) | §3.14 SW-1, SW-2, SW-8; "v3.1 notes" | B (M4) | T87, T89, T93 | M4 |
| **PE-7** | Shard court (audit `8be0f661`, launch line only, never the live fleet): under the fence a one-move verdict goes through `palw_one_move_verdict_bound_v2`, and `NeedsDissection` is refused for classes whose root evidence cannot be built (held-context classes); the seat-side ladder fix passes the class ladder | J-8 | A | the audit's tests on `8be0f661` | launch line |
| **PE-8** | **An open gap that gates launch:** an arithmetic lie in an `AttnFused` step leaf on a held-context class (8k/2M) has no conviction route. The audit is doing the design pass on (A) a chunked attention proof, (B) K/V rows as committed outputs plus held-DA, (C) an economic bound; (B) would touch producer/engine commitments and P2-7's retention duty | §3.9, §4.1 (ii), §4.2 #18, J-6, §8.3 item 7 | A (design); B (P2-7 if (B)) | named by the chosen design | Gate |
| **PE-9** | The S spec (`s_spec.md` lines 17–145) is the v22 table source for S: `claim.rcore` (`PalwClaimRcoreV1`, byte-identical to rows 2–6); the liability's `licence_door` (optional), `basis_k`, `g_res_sompi`, `escrowed_reward`; the lock's `attested` and `segments`; the consumed offence's `collected` and `claim_id`; void reasons `UnavailableQuorum = 5`, `NotReplayBacked = 6`; `PalwLicenceDoorTagV1`; objects 53–56; delta entries from 66 in landing order (S's five, then B's vesting entries, then M3's two); the rooted maps; `palw_rcore_plus`, `palw_rcore_conservative_classes = [2M]` and three `#[borsh(skip)]` mirrors; landing order M1 → M2 → S → M3 → M4, frozen at M5 by T41. The vesting rows, step 3d, `VestingNote` and A-KEY are B's, landed in S's window | §6, §3.3 (owner note), §3.5 A-1 (note), §7.1 S | A (S); B (vesting) | T24, T40, T41 | S |
| **PE-10** | Audit flags: (a) `palw_claim_commitment_v1(params, claim, now_daa)`, three arguments, supersedes `phase2-plan.md` §2.3; (b) "byte-identical" holds for behaviour and ids, not encodings (v22 moves encodings everywhere; the V2 params id re-pins once); (c) L-4b extends to FP (`escrowed_reward` is 0 on FP; `lock_2` prices `rr`); (d) the B-3 exit bound includes the DA lattice (`palw_v2_bond_withdrawal_delay_at_v1`) | SR-1, `phase2-plan.md` §2.3; §6, SW-1, T24, T88; L-4, L-4b, T78; B-3, R-2, T23, T42 | A (S); B (the plan) | T24, T78, T23, T42 | S; P2 |
| **PE-11** | **P6 (B; a hard precondition for any drill of an S-bearing build):** kaspad's producer pre-check (`palw_panel.rs:2438`) uses `palw_producer_facts_v4(.., raw_depth).committed` **and** the U2 floor gate, so producers never mine attempts the fold refuses. The S-1 symbols: `palw_bond_producer_floor_shortfall_v1(state, params, bond, now_daa) -> Option<u64>` (`None` = meets the floor or dormant; unknown bond = `Some(floor)`) and `palw_bond_meets_producer_floor_v1`; current posted collateral after slashes, not free stake; dormant unless `rcore_plus_from_daa`; the fold's own-attempt refusal is the non-fatal `PalwStateV2Error::ProducerBelowFloor { bond, collateral, floor }`; kaspad logs "holding: top up <shortfall> sompi to reach the producer floor" | B-4, §7.1, §7.3, §8.3 item 3 | B (P6); A (S-1's symbols) | T08, T17 | S-1; before the drill |
| **PE-12** | User decisions on S. **U1:** the escrow is held to Final only for C7 (2M); 8k releases per SR-1. **U2:** ejection = the capital predicates **plus a producer floor gate** (posted collateral ≥ `min_collateral_sompi`, 13,000 MSK on t12), checked at attempt admission (ADM and the fold's `apply_attempt`, the own attempt's refusal non-fatal like `AttemptExposureCeiling`) and for FP executors at commitment; no status, no tombstone; an honest 13k producer after one S0′ (−3,200.95) tops up before producing again (no in-place top-up exists: re-registration under a new key and operator identity until §9.3 Q11 decides); kaspad reads the same predicate (P6). **U3:** an FP claim convicted after Final charges the executor a producer action tier capped at `m × G_fp` (G = the liability's `g_res_sompi + escrowed_reward`); the signers' S4 and the root forfeiture stay. Both land in S (S-3, S-4), with no new v22 field | SR-1 cond. 4, B-4, §2, §3.6, §4.4 | A (S-3, S-4) | T01, T17, T22 | S |
| **PE-13** | The licence-stall follow-up: after the licence-stall fix an optimistic licence credits at most 2 seats (the full seat and at most one partial) unless all five validated, when all five ride and the coverage door normally takes that set first (`palw_select_optimistic_licence_v2`, `palw_panel_v2.rs:1886` at `c8652a97`; licence-stall report Fix 1: never 3–4 `Valid`s), and t12 has no supplementary path for V3 receipts; **SR-10 (the V3 supplementary door) is in M4 (B)**, so an S2 claim upgrades only from M4 on | SR-10, SR-1b, Q-5, §6 row 24, §7.1, T74 | B (M4) | T74 (V3 half), T72 | M4 |
| **§3.14 sync** | SW-1…SW-9 brought in line with rows SW, SW-A1…SW-A6, R2 and R5, which only the tables had absorbed: the weight capped at `PALW_DRAW_WEIGHT_CAP_MSK_V1` (no relative-cap text); SW-2's first reason withdrawn; the admission jury ADR-0147's (the weighted jury deferred, §9.2); SW-8 rewritten (one state, bind only in the anchor block); SW-9's "availability, not safety" corrected; SW-A6's `ready_eff` residual named; **SW-10** (875‰ eligible-stake floor, `InsufficientEligibleStake`); code anchors for P7; §4.3's rows for 7…0 of 8 eligible genesis seats; T85–T94; "v3.1 review disposition" added | §3.14, §4.2, §4.3, §6, §8.1, §8.3, §9.2, §9.3 | B (M4) | T06, T85–T94 | M4 |
| **Review round** | The post-edit review's corrections: (1) U2's "top up" has no mechanism in the code (append-only registry, `DuplicateBond` / `DuplicateBondKey` / `DuplicateOperator`, the slash the only `collateral` write), so today it means re-registration under a new key and operator identity, and the in-place alternative is open as §9.3 Q11; (2) the addendum's seat fixes are SEAT-S1…SEAT-S4 (S-0…S-8 stay S-SPEC's commits), with owners in §7.1 "Seat fixes"; (3) contradiction 12 carries the addendum's hash-form payload, and 9's site follows Q-6; (4) R-core's claim fields are written `claim.rcore.<field>`; (5) the X10 field is declared with M6, not at genesis; (6) an optimistic licence carries all five receipts when all five validated; (7) SW-10's tolerance column respects the 5-operator minimum; (8) the launch-line room fix is B's | B-4, SR-1, SR-5, SR-10, J-5, Q-5, Q-6, Q-7, SW-8, SW-10, §6 row 20, §7.1, §7.2, §8.1, §8.3, §9.3, 運用者向け要約 | as in each section | T17, T18p, T18p-M, D-10 | as in each section |

---

## Integration amendments (2026-09-24)

The integration line (`rcore/int-2`; header "Line references") implements this ADR with the changes below, and the
operator decided further points on the same day. Each item is applied **in place** in the sections named, and names
the code symbol that implements it so a reader can check it. Owners as in "v3 changes". Figures come from the code, its
doc comments or its tests, or from the sources named in the item; nothing here was measured on a live claim.
"Pending" marks a rule decided or required but not on the integration line.

| item | what changed, and why | where | code of record | tests | source |
|---|---|---|---|---|---|
| **IA-1a** | **SW-8's anchor block is an attempt block past `palw_rcore_plus`.** The anchor walk and the fold read one predicate: the attempt lanes only (algo 6 and 9); below the fence every lane but the chain-positionless ones, as before. A claim's anchor block is the first attempt block at or past its slot `bind_base_daa() + anchor_delay`, and it binds there or voids there (IA-1c). **Why:** a heartbeat anchor let the anchor's producer re-roll the stake draw's seed at the heartbeat target, 2^24 hashes per try, instead of an inference (an attempt's execution commits to its header's pre-PoW hash). **Liveness residual:** a claim waits for the next attempt block; if none reaches its slot before `bind_base + window_bind`, it voids `BindTimeout` at that backstop, S0. So a heartbeat-only stretch voids, without forfeit, every claim whose slot falls in it. The lattice keeps `anchor_delay + max_beacon_gap` = 20 + 400 = 420 < `window_bind` = 600 (`PalwConsensusParamsV2::validate`, as the resolver's doc states). **Considered, not taken:** anchoring on a committed seed after K heartbeats (§9.2). **Rate:** no source gives an attempt-block rate for public t12; the bind window is to be derived from the rate measured after launch (O-13). | SW-8, DL-1, §8.4 O-13, §9.2 | `palw_block_may_anchor_a_panel_v1`, `palw_sw8_anchor_delay_for` (PROC); `PalwTransitionExtrasV1::sw8_anchor_delay`; `palw_claims_provisional_past_their_anchor_slot_v1`, `palw_void_claims_unbound_in_their_anchor_block_v1` (step 4c, STATE) | `t12_stake_draw_integration` (`sw8_the_anchor_block_voids_a_claim_it_does_not_bind`: the heartbeat at the slot resolves `None`; T89) | `f422df49`; the resolver's doc |
| **IA-1b** | **SW-10 counts the executor's weight on both sides.** The rule is `(E + X) · 1000 ≥ (B + X) · 875`, with X the executor operator's SW-2 weight (capped) where it could sit on the class but for being the executor, else 0; the outsider's floor takes the same term. **Why:** t12's producers are its genesis cards, so a genesis card's claim draws from the other seven; one saturated seat then gave 6/7 = 857‰ < 875‰, and one busy card could halt every other card's binding. With X a genesis executor is back at 8 (7/8 binds, 6/8 refuses). **§4.3:** in total attacker capital no threshold moves (X relaxes the floor exactly as the same capital posted as an idle Sybil would, and buys no seat). Counted in Sybil stake, §4.3's unit, a floor-bound row falls by at most X ≤ 1,000,000 MSK. The free-redraw worst becomes **6.63M** (51 operators; 6 of 8 eligible beside an executor bond posted at the cap), was 7.15M (55; 7 of 8). P2 with filing's worst stays 12.74M. 5 of 8's floor-bound entry becomes 14.04M (108), was 15.08M (116). This 6.63M is **a separate quantity** from the two other 6.63M figures in this ADR (P3's design-point threshold, §4.3, and the weighted admission jury's even-odds stake, SW-5), and from the held-attention bounds (§3.9) | SW-10, §4.2 #13, §4.3, §4.4, T06, T94, 運用者向け要約 | `palw_panel_stake_executor_bonds_judging_v1` (its doc gives the figures), `palw_panel_stake_race_with_v1` (DRAW) | T94 at fold level (the genesis-executor tolerance); `rcore_s3_one_ledger.rs` (7 of 8 binds with the executor term, 6 of 8 refuses `InsufficientEligibleStake`); the outsider case in DRAW's module tests | `f422df49` (M4 review finding 1) |
| **IA-1c** | **`BindTimeout` is applied in the anchor block (step 4c)**, at that block's own DAA. That is the exact void DAA DL-1 left to M4. It is S0: `void_claim` releases the producer's reservation (no seat is on duty for an unbound claim), and the class's in-flight count and rate-room demand come back in the same block. A free-prompt claim starts its abandon hold there (SR-1). A **saturation void** (`InsufficientEligibleStake`, SW-10) is one of these: no forfeit, released at once. Nothing new is stored: the deadline index keeps `bind_base + window_bind` as the backstop, and the rebuild and `assert_deadline_consistency` reproduce the transition unchanged. **Pending:** a test that asserts the producer's `reserved_exposure` falls at that void (the integration to-do: P6/S) | DL-1, SW-8, SW-10, T89, T94 | step 4c; `void_claim`; `bind_timeout_reason` (STATE) | `sw8_the_anchor_block_voids_a_claim_it_does_not_bind` (no collateral or `slashed` moves; `assert_deadline_consistency`) | `f422df49` |
| **IA-1d** | **`palw_rcore_plus` requires 14 fences** at or below it, `palw_audit_2026_09_11` added: SW-8's one state is the acceptance walk's object-by-object pre-object base, which exists only past that audit's A-1. The validator also refuses a non-genesis `palw_rcore_plus`, a missing `palw_settled_anchor_depth`, `palw_shard_licensing` beside it, a non-V5 context root, and a panel quorum other than `PALW_PANEL_COLLUDING_QUORUM_V1` | §6 prerequisites, T24 | `Params::validate_palw_rcore_plus_v1` (`params.rs`) | T24's per-prerequisite negative cases | `f422df49` |
| **IA-2** | **A-6 is one invariant at every gate past `palw_rcore_plus`: `committed + accuser ≤ C`.** Its two gates, as one pure function computes them: **work** (a claim, a duty, a lock top-up, an FP commitment) has room `min(ceiling − committed, C − committed − accuser)`, that is `committed + new ≤ 500‰·C` **and** `committed + accuser + new ≤ C`; **accuser** (a court, a held dissection, a DA accusation) has room `C − max(committed, ceiling) − accuser`, that is `max(committed, 500‰·C) + accuser + new ≤ C`. So accusers use only the free half and never the work half's unused room, and a later claim can never be what breaks the invariant. One number serves the fold, admission, the draw, the producer's facts and the RPC. The producer's facts apply the work gate's inequality as `has_committed_room`, which `ready_to_produce_v3` and kaspad's P6 read | A-3, A-6, B-3, SR-7, P6 | `palw_rcore_gate_room_of_v1`, `PalwRcoreGateV1 { Work, Accuser }`, `palw_rcore_gate_room_v1`, `palw_bond_headroom_v1`, `palw_accuser_room_v1`, the fold's `gate_room` (STATE); `PalwProducerBondFactsV2::has_committed_room` (`palw_producer_v2.rs`) | `the_work_and_accuser_gates_keep_one_invariant` (STATE lib); `rcore_one_invariant` (`m1_the_invariant_holds_across_interleavings`); `m1_a_da_accusation_is_refused_past_the_accusers_free_half` | the S review's M1 (`0b56c4d8`); `c3fe99cd` routes M3's DA check through it |
| **IA-3** | **IMPL-15 and the S re-review: `g_res` carries the buyback bound `s`, and the vested lock prices `s` at its cap.** `G_res = w + R + s`, where `s` is the ADR-0091 slice, 5% of `E`, when the claim's line has a pair open to the buy, else 0. `0b56c4d8` reverted an `s = 0` deviation. The lock the fold posts once the vesting rows land takes `s` at its **cap**, `5% · E`, whatever the pair's state at the licence, because a pair can open between the licence and the `Final`: it is L-1 on `g_res − s + s_cap` with the escrow term on `E − s_cap`. While the rows had not landed, the lock was priced on the whole gain, with `s` inside `E` | §3.6 "G, as a function", L-1, §2 values note | `rcore_buyback_bound`, `rcore_g_res` (the fold view); `palw_rcore_lock_v1`, `palw_rcore_lock_vested_at_cap_v1`, `palw_rcore_lock_vested_v1`, `palw_rcore_lock_unvested_v1`; `palw_model_buyback_slice_v1` (5%) | `rcore_whole_gain_and_buyback` (`h1_…`: the fold's lock is the at-cap price; `m2_…`: an open pair raises `G_res` by `s` ≈ 160 MSK on 8k, and the armed price is the same with the pair open or closed) | `0b56c4d8`, `479cdfa3` |
| **IA-4** | **Court time can debit a losing challenger up to twice what A-6 reserved** (a note on existing pricing, not a change). A-6 reserves the court's `claim.reserved` on the challenger's accuser ledger. A challenger-side close takes that reservation (`slash_seat`, capped at `min_collateral_sompi`), and the deep fence's court-time charge takes up to `claim.reserved` more, in proportion to how long the session ran. The second part is taken by the saturating `slash_bond` from whatever collateral the bond holds, not from reserved room | A-6 | `rearm_after_challenger_side_close`, `charge_court_time_v1`, `slash_seat` (STATE) | — | C-03 (the 2026-09-11 audit's deep fence) |
| **IA-5** | **The licence door does not determine `basis_k`.** The recorded door says how the claim reached `basis_k ≥ 2`, not what `k` is: an S2 licence that supplementary sets raise to `basis_k ≥ 2` is re-recorded **Coverage** if a counted mask is partial, else **Quorum**, with the recounted `basis_k`: 2 when one seat is added, up to 3 when one set adds two (`stage_supplementary_v1` takes `min(3, old_k + added)`, 3 = `PALW_PANEL_COLLUDING_QUORUM_V1`). So a Quorum record can carry `basis_k` 2, and the door's name never sets `k`. L-1 prices `lock_{max(basis_k, 2)}` by the recount, never by the door. **SR-1b is one rule for both supplementary doors**, S-2's V2 door and F4's V3 door, and the flip moves the producer's ledger in the same write (SR-3). The masks recorded on a lock: a V1/V2 `Valid` the full cut, a V3 `Valid` its own mask (S-3). A lock with no mask (`segments == 0`) reads as the seat's assignment, which can only under-count. **`Sampled`** (Q-1) counts in no quorum, coverage or upgrade, takes no lock, latches `unserved_seen`, and is credited for pay | SR-1b, SR-2, SR-3, L-1, L-3, Q-1, Q-3, Q-5 | `palw_rcore_supplementary_flips_v1`, `stage_supplementary_v1` (V2 door), `credit_supplementary_receipts_v3` (V3 door), `move_commitment`, `staged_licence_v1`, `palw_rcore_counted_masks_v1`, `palw_rcore_release_due_v1` (STATE) | T74 (both halves; `sr1b_the_v3_flip_moves_the_escrow_term_off_the_producer_ledger`), t71 (V2/V3 recount agreement), `palw_rcore_m4_sampled_and_the_v3_door`, `t12_rcore_sr10_door_gate` | `6b5af6f5` |
| **IA-6** | **The vesting contract, as the vesting work ships it** (`phase2-plan.md` §2.3/§2.4). The planner takes three arguments, `palw_vesting_mint_plan_v1(state, budget_new_keys, market_waiting)`, and step 3d calls it on the state it plans. The RPC and the Phase 2 coinbase harness call **`palw_vesting_next_block_plan_v1(state, params, next_daa, raw_depth)`** on a committed state: it replays the next block's 1b drain and 3d latch, and equals the next fold's plan unless that block itself moves step 3d's inputs. The budget helpers are `palw_vesting_budget_v1` and the market/non-market queue counts. The note tags **5 `BuybackAtFinal`** and **6 `ReserveCredited`** close V-3's identity from deltas. A row's position is counted **in keys**: `palw_vesting_mint_position_v1` returns (moves ahead, keys ahead). Step 3d moves a reporter reward **in the block the sweep wrote it** when its budget has room | V-3, V-7, §7.3 | `palw_vesting_v1.rs`: the functions named, `palw_vesting_market_rows_waiting_v1`, `palw_vesting_market_rows_waiting_after_drain_v1`, `palw_vesting_non_market_rows_waiting_v1`, `PalwVestingNoteV1`; `apply_vesting_maturity` (step 3d) | `vesting_fold_v1`; `the_next_block_plan_is_the_next_folds_plan_over_a_simulated_chain`; T03, T16/T30, T47, T82 | `d3ece0d5` |
| **IA-7** | **`PALW_RCORE_VESTING_ROWS_LANDED_V1` is `true` on the integration line, and why it may be.** `finalize_claim` writes the row (V-2), `burn_vesting_row` burns it, and **every post-Final conviction reaches that burn through S-4's funnel**: S3 for kinds 3 and 4, for a proven court verdict and for the FinalRow DA default; U3 for a free-prompt claim, which writes no row. Before the funnel only the DA default burned a row, and the flag stayed `false` for that reason: pricing the residual `G − E` without the burn under-collateralizes. **kaspad refuses to start** a network that arms `palw_rcore_plus` while the flag is `false` (N1) | V-2, V-5, L-1, §3.3 ownership, §6 | `PALW_RCORE_VESTING_ROWS_LANDED_V1`, `write_vesting_row_at_final`, `burn_vesting_row`, `post_final_producer_leg_v1` (STATE); `palw_rcore_build_can_run_v1`, `ConfigError::PalwRcoreVestingRowsNotLanded` (`kaspad/src/daemon.rs`) | `rcore_whole_gain_and_buyback` (asserts the flag), `dos_l5_4b` (no stall with the flag), `dos_l5_6` (`daa_2m_dead == 0` with the flag), `n1_rcore_plus_needs_the_vesting_rows_at_startup` | `479cdfa3` (N1); the S-4 merge `68f0d672` (the flag set `true`) |
| **IA-8** | **G is fixed at the fraud.** `claim_g_v1` reads the **recorded** `g_res_sompi` and `escrowed_reward` of the claim's liability row wherever one exists, never the live facts, whose realizable-rights term shrinks as the claim's rights are realized. That shrinkage would lower every action priced on G the longer a conviction waits (on `rcore/int-2` T66 measured the gap at 1,000,000 sompi). Only a claim with no row is priced from its live lock facts. `persist_panel_liability` writes the row at the `Final` and at a void. So between licence and `Final` a claim is still priced live. **Pending (the audit's fix):** a `G_res` recorded **at licence** in `claim.rcore` (a new v22 field, `PalwClaimRcoreV1.g_res_sompi`), so the L→F interval is fixed too. It is not on the integration line and is marked pending in §6 | §3.6 "G, as a function", §6 row 8 | `claim_g_v1`, `persist_panel_liability`, `PalwClaimGV1` (STATE) | T66 (`rcore_m3_da_court`), `rcore_s4_conviction_funnel` (`g_of` reads the row) | `672436d8` (the gap, flagged to the audit); the S-4 merge `68f0d672` (the row read first, the audit's interim half) |
| **IA-9** | **S-4, the conviction funnel, as implemented.** Every conviction first **opens** a record for each bond it may charge: `C₀` before its first debit, and whether that bond's exit gate was shut on the pre-state. Its legs then run through `slash_bond`, which returns its debit. It **closes** with the consumed record written last: `amount` the nominal tier summed over the legs, `collected` the debits of the bonds whose gate was shut, `claim_id`. The tiers are listed in §3.6. S0′ opens no conviction: it is the forfeit alone, with no strike, action, record or reward. Four rules, each from its own source: **(1) the operator's decision (2026-09-24, S-4 deviation 2):** a court **default** is charged the forfeit **plus** S2's `min(10%·C₀, 3G)`, so silence is never cheaper than losing (M2's rule charges `CourtDefault` as `CourtFraud`), and it writes **no** `CourtConviction` record; **(2) S-4's implementation (`c3fe99cd`):** a kind-3 record's `amount` and `collected` **include the producer's leg** when the finding acts on the claim; **(3) the audit (its #12, in `convict_false_valid_rcore_v1`'s doc; v3.1's DA-7 already names covering signers by a live lock):** S4's action applies **only while the seat holds a live lock**, so a seat the liability row lists without one is convicted for 0; **(4) a correction of v3.1, read from `palw_rcore_strike_v1`:** the strike list can hold **up to 9** entries, not 8 (arithmetic, not a decision) | §1.5, §3.6 (tiers, the funnel), DA-7, §6 row 16, T22, T35 | `open_conviction_v1`, `close_conviction_v1`, `conviction_collected_v1`, `void_and_slash_at`, `convict_false_valid_rcore_v1`, `consume_executor_refuted_v1`, `convict_by_court_verdict_v1`, `record_court_conviction_v1`, `convict_equivocation_rcore_v1`, `post_final_producer_leg_v1`; `palw_rcore_s1s2_action_v1`, `palw_rcore_s3s4_action_v1`, `palw_rcore_eq_cap_v1`, `palw_rcore_strike_v1` (STATE) | `rcore_s4_conviction_funnel` (S1 strikes; S0′; T76 Eq; R-2 gate-open and pre-drained; T81; `s2_a_court_default_is_charged_as_a_fraud_and_writes_no_court_conviction`); the lib's `s4_conviction_funnel`; T46 on real claims | `c3fe99cd`, merged at `68f0d672`; memory `adr0152-v2-user-decisions-f1-f4-gate` (rule 1) |
| **IA-10** | **The reporter reward, as S-7 ships it.** Commit–reveal over tags 53/54. **R-4 is proven-only:** only a proven conviction opens a reward, by basis: `CheckedEvidence` (kinds 0, 3, 4), `CourtVerdict` (kind 6) and `DaDefault` (kind 5). A court default (`CourtDefault`) opens none. A DA default pays the accuser of the earliest defaulted session, on the producer's collected debit only (a recorded decision in the seam's doc: the offence is the withholding itself). DA-7's charge hook opens it, wired at integration | R-3, R-4, DA-7 | `open_reporter_reward`, `PalwConvictionBasisV1`, `palw_reporter_commitment_v1`, `da_default_charge_v1` (STATE) | T39, T75, `rcore_m3_da_court` T18 (the DA-default reward; none with the fence off) | `1f4b2b2a`, `672436d8` |
| **IA-11** | **M3 deviation 5: a held unit does not bind partial-mask signers.** Placing a held step leaf in a segment needs the claim's committed step-leaf count, which no record the fold keeps carries after the accusation. It would need a new v22 field, and the operator decided partial seats are not bound at launch. So DA-7's signer S4 charges **full-mask signers only**, for event and held units alike. That under-charges a colluding partial seat and never over-charges an honest one. The signer half stays dormant until the seats' DA answering lands (IA-14) | X7, DA-7, Q-6, §4.2 #16 | `palw_da_unit_covered_by_v1` (`palw_da_rcore_v1.rs`); `palw_da_signer_liability_armed_v1`, `PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1` = `false` | T66, T73 | `f64870c9` (deviations), `479cdfa3` (kept) |
| **IA-12** | **2M is closed at launch (U-D1)**, and the held-attention and deadline decisions are recorded: 4-ter's A-held for 8k in the gate; 4-quater's class-derived deadlines with the kinds R/C/E/K; U-D1…U-D10; **P-1, the pruning depth fixed at regenesis at ≈ 74,920 DAA (D_cap 16,000), without waiting for M12** (operator: drills and measurements after launch). **Pending:** 4-quater's consensus rules and P-1 are not on the integration line (no `palw_class_verify_deadline` there). Its node items N-1, N-4 (U-D10), N-5 and N-6's R2 ship in SEAT-R and the seat work | §1.5, §3.9, T-2(b), §4.2 #18, §8.3, §8.4, §9.1, 運用者向け要約 | node: SEAT-R `b589622f5` (N-1, N-4), `1e20edd50` (N-5, R2, N-9's display) | — | memory `held-attention-attribution-8k-gate-2m-cap`, `t12-drills-after-launch-implementation-first`; the audit's `deadline_2m_design.md` §1.4, 4-quater |
| **IA-13** | **SEAT-R/SEAT-S residual, a 2M flag-day item.** The family checks a served sibling path only after the replay, so a holder of a genuine opening can spend a segment's bound with forged paths. That bound is `PALW_SEAT_S4_CANDIDATES_PER_SEGMENT_V1` = 8, and each forged path costs a whole segment replay in the one C7 slot. The seat then abstains in silence (served, never accusing). The remedy is a family (base0/core) API change: return the recomputed range fold, or take several paths. C7 is closed at launch | Q-7, §8.3 | `PALW_SEAT_S4_CANDIDATES_PER_SEGMENT_V1`, `PalwSeatResumesV1` (`kaspad/src/palw_panel.rs`) | `h1_a_junk_or_forged_opening_does_not_stall_or_fault_an_honest_claim` | `1e20edd50` (the residual) |
| **IA-14** | **Ship conditions**, a checklist (§8.3 "Ship conditions"). F2's and F1's fences ship only with SEAT-S1, SEAT-S2 and SEAT-R. SEAT-S2's kaspad half compares `output_root` under `CoreV1` in the production replay step, tested on a real model-class replay. The `AnyValid` fence (M2 Phase 3) also needs H-1 and H-2 of the audit's M2 Phase 3 review (the fixes are B's). **H-1 is met on the integration line:** past SEAT-R a partial seat's resume abstains before the S3 layer-sample arm. **H-2 is pending** (the FP S1 resume's job-pin check, which needs `d675423d`). **The producer's V2 DA responder ships unconditionally** in every build that arms `palw_rcore_plus`, because DA-7's producer charge does not wait for the flag. `PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1` goes `true` only with the seat DA auto-answer (P2-7), which arms the signer half. `misaka-palw-derive` and the stranger script recompute `output_root` under the network's attempt rule | Q-7, §7.2, §8.3 | `palw_seat_replay_step_v1`, `palw_v2_try_partial_resume_v1`, `palw_seat_r_in_force_v1` (kaspad); `da_default_charge_v1`, `palw_da_signer_liability_armed_v1` (STATE); `palw_da_duties_v2` (`palw_producer_v2.rs`); `palw_attempt_rules_of_params_v1`, `palw_attempt_output_root_v1` | `449fd892` (the real model-class replay); `c1_s3_never_attests_past_the_fence_without_verify_material` (H-1); `a4682a8d`'s derive and stranger-script checks; the responder's end-to-end test is to be written | `449fd892`, `1e20edd50` (H-1), `a4682a8d`; the integration to-do (H-2, P2-7, the responder: the M3 review's F2) |
| **IA-15** | **P6, integrated.** The producer asks the one ledger and the producer floor, through **one readiness verdict** for the node and the RPC, `ready_to_produce_v3`. Past the fence it checks, in `apply_attempt`'s order: key and bond, the producer floor, the class gate, the epoch budget, the committed room (`has_committed_room`). kaspad adds only the floor hold's top-up sentence and, last, the SW-10 stake question. `getPalwProducerFacts` reads the same verdict. Its wire v9 adds `bond_committed`, `bond_producer_floor_shortfall` and `bond_accuser_exposure` (gRPC tags 31–33). The FP price answers the fold's `ProducerBelowFloor` first, then the T-2(a) share refusal. **Pending:** the SW-10 stake question has no consensus read yet, so kaspad's seam answers "unknown", which never holds | B-4 (P6), §7.1 P6, §7.3 P6 | `PalwProducerFactsV2::ready_to_produce_v3`, `PALW_NOT_READY_BELOW_PRODUCER_FLOOR_V2`, `PALW_NOT_READY_REASONS_V2` (`palw_producer_v2.rs`); `palw_producer_ready_v1`, `palw_class_eligible_stake_at_floor_v1` (kaspad); `palw_fp_commitment_price_impl` (PROC) | `ready_to_produce_v3_reads_the_floor_and_the_committed_ledger_past_the_fence`; wRPC v9 and gRPC round trips; CLI operator tests | `42ef9642`, `0613580f` |

**What the integration line does not have yet** (at `8e1ce34a`; each is a ship condition or a named pending item,
§8.3; branch heads as of the amendments' review):
* 4-quater's consensus rules (`palw_class_verify_deadline`: V1–V6, K-1, the 2M closure V2) and P-1's pruning depth
  (IA-12). No symbol of that fence is on the integration line, so U-D1's closure is not yet a consensus rule there;
  what bounds 2M there is C7 (`c_2M = 1` held to Final, T-2(b)) and RT#2's forfeit.
* A-held (4-ter's C1–C5 and object 57) and the shard court `8be0f661` (J-8). Both are on the audit's `feat/t12-aheld`
  (`2a298d33`, over `88e75207`, which merges `8be0f661`), under review and not merged into the integration line.
  B owns three pieces of it: kaspad's routing of held classes (N3), the producer's automatic object-57 answer within
  42 DAA, and the N4 wiring of `with_offence_attribution_v1` at the SDK and lineage constructors. N4 is mandatory: its
  selector is off by default, and a node without it would produce 8k claims it cannot defend (§3.9).
* **M2 Phase 3** (the audit's `feat/t12-m2-f1`, under review): the 3a review fixes `d675423d`, 3b `aae5d3d6`
  (contradictions 11–13, the heavy budget, `AnyValid` for 9's job faults and for 13), 3c `f3bd0fa8` (a class must be
  attributable to register past `palw_offence_attribution`; the court door), 3d `9b4249f2` (the drill hook
  `PalwDrillFaultV1` and T18p-M), and the Phase 3 review fix `493696de`. None is an ancestor of the integration line,
  so contradictions 11–13 (F1-M's and F1c's `ForgedOutputTiled`, `LogitsNotStepOutput`, `PromptNotAnchored`), the
  `AnyValid` sites, T18p-M and the drill hook are not there (§8.3 items 1, 6 and 9).
* **The `AnyValid` fence's H-2** (IA-14): the FP S1 resume's job-pin check, B's, which needs M2's `d675423d`. H-1 is
  met on the integration line (SEAT-R's abstention, §8.3 item 9).
* **S-5** (S-SPEC §7): SR-9's object 56 `PanelUnavailableQuorum`. The fold still refuses it by name
  (`RcoreObjectNotLanded`; `palw_rcore_plus` "arms 55 … and not yet 56 (S-5)"), so SR-9 cannot fire and drill step
  D-8 cannot pass on this line. S-5's SR-10 V3 door arrived with F4 part 1 (`36303abe`); its `redraw_claim` and the
  licence-predicate fix were not re-checked here.
* **S-8** (S-SPEC §7): the round-trip battery, the fence-off twins, T38 and the test moves.
* **F4 part 2** (`rcore/f4-part2`, no commits past the integration line): Q-5's gate (DL-1's `basis_k < 2` row, the
  first-panel redraw, `NotReplayBacked`, the upgrade's deadline re-arm) and Q-7 on kaspad (S3 → `Sampled`, a V3
  supplementary collector), as `36303abe`'s message leaves them.
* **Phase 2's node stages** (`phase2-plan.md` §3). P2-1's B-3 call sites **are** on the line: both UTXO sites read
  `palw_bond_collateral_is_locked_v6` through `palw_v2_bond_is_locked` (S-1, `849c0dd4`); and the vesting work
  (`d3ece0d5`) carries the planner and step 3d (IA-6). Not merged: **P2-7** (the seat DA auto-answer, with the flag
  set) on `rcore/p2-panel` (`b34df2ff`, `fc5e30d8`); **P2-10/P2-11** (`getPalwVesting`, the CLI) on
  `rcore/p2-rpc-cli` (`d64f112f`, `9ca9de9c`). P2-6, P2-8, P2-9 and P2-12 have no branch yet. P2-2…P2-5 were not
  re-checked against §8.3 item 5's tests here.
* **The producer's V2 DA responder** (IA-14; the M3 review's F2, CRITICAL). Past the fence kaspad still builds the v1
  answer objects, which the fold refuses, and `palw_da_duties_v2` lists only `DefaultDisputed` duties, so no R-core
  DA session is answered while DA-7 charges the producer (the integration to-do's launch blockers). `rcore/p2-panel`'s
  duty list covers the node's own claims; it is not merged.
* The licence-time `G_res` field (`PalwClaimRcoreV1.g_res_sompi`, the audit's commit; IA-8), and a test that the
  saturation void releases the reservation at once (IA-1c).
* The SW-10 read for the producer's seam (IA-15).

**Corrections after the amendments' review (2026-09-24).** One reviewer read the first amendment pass (`fe01a04f`)
against the integration line at `8e1ce34a`. Each point is applied in place:
1. H-1 of the `AnyValid` fence is met on the integration line; only H-2 is pending (§8.3 item 9, IA-14, the list above,
   the header).
2. The producer's V2 DA responder is an unconditional ship condition, separate from the flag that arms the signer half
   (§8.3 item 9, IA-14, DA-7, the operator summary).
3. The anchor is `8e1ce34a` (the S-4 merge `68f0d672`), not a working tree (header, IA-7, IA-8, IA-9, the Appendix).
4. The list above names every R-core+ part the line lacks: M2 Phase 3, S-5, S-8, F4 part 2 and Phase 2's node stages
   (the header no longer says "implemented"). P2-1's B-3 is already on the line (`849c0dd4`), so it is not listed.
5. IA-9's S-4 rules are attributed to their sources; only the court default is the operator's (§1.5, §3.6, §9.1).
6. The operator summary, §2's amplification row, §4.2 row 7 and §4.4's lock, duty and row-7 figures say they are at
   `s = 0`.
7. S0′ opens no conviction and writes no record (§3.6's funnel and tier table, IA-9).
8. An upgraded S2 claim records `basis_k ≥ 2`, 2 when one seat is added (SR-2, Q-3, IA-5).
9. M13 is B's (`3e6a2220`); the 2026-09-23 heartbeat scan is the audit session's observer; O-13 has no cadence.
10. O-11 follows U-D1; §8.3 item 3's short drill moves after launch (the operator, 2026-09-24, §9.3 Q12 (b)).

---

## v3 changes

v3's table, kept, with the rows that v3.1 re-spells updated in place (F1, F2, D4, C6, IMPL-3, IMPL-13, IMPL-18, C12).

Every review finding (F1–F21), every operator decision of 2026-09-24 (D1–D7), and every Phase 2 plan finding
adopted here, mapped to the rule that closes it.

**Phase column.** The mandated order is **M1** F2 → **M2** F1 → **M3** F3 → **M4** F4 → **M5** fold/reorg/restart
tests → **M6** X10 (a post-launch fence). **S** is the Phase 1 skeleton (bond, staged reserve, vesting rows,
fence, schema v22), which the audit session lands after M2. **P2** is Phase 2. **O** is the post-launch
observation program (§8.4).

**Owner column.** **A** = the audit session (Phase 1: M1, M2, S, M3). **B** = the ADR owner (Phase 2, F4 and, since v3.1,
the stake-weighted draw, both in M4).

| item | what it says | closed by | owner | tests | phase |
|---|---|---|---|---|---|
| **D1** | Keep RT#2's full forfeit until F1, F2, F3 are GREEN; a temporary cap is allowed, a free timeout never; X10 arms later by fence | SR-5 (S0′ on the second failed panel; the FP abandon hold kept), §6 `palw_rcore_attributed_charging` | A (S), then A/B (M6) | T02, T02b (fence-crossing twin), T02c | S; M6 |
| **D2** | S3 Valids never count toward a quorum; a quorum Valid is a full replay or S1 with sufficient coverage; S2 is a fast path, never the basis for Final; sample-only signers are not slashed after Final | §3.12 Q-1…Q-7 (S2 licences do not tick the second clock, V-8) | B (M4), A (fold review) | T70, T71, T72, T72b, T73 | M4 |
| **D3** | The DA court redesign goes into this ADR: drawn units, no monopoly, after licence and after Final, pause credit, Incapable ≠ served; **only a Valid serves** | §3.11 DA-1…DA-9; SR-1 cond. 2 (`Sampled`, `Incapable`, `Unavailable`, missing all hold) | A (M3; SR-1 in S) | T27, T64, T65, T66, T67, T68, T69 | S; M3 |
| **D4** | m = 1 dropped for m ≥ 3; safety rests on independent evidence, not on S2/S3 sampling; producer floor stays 13,000 MSK | §3.6 (m = 3), §3.12 Q-3/Q-5, §4.3 (undetectable success = both assigned attesters of the lied segment, every door; **v3.1: under the stake-weighted draw (§3.14) EV-positive from 17.29M MSK of Sybil stake at the design point (133 × 130k operators beside the eight genesis seats) and from 12.74M in the worst saturation state SW-10 admits; 8.32M / 6.63M (7.15M before IA-1b's executor term) if a failed first panel's honest attester files nothing**; v3's uniform figures were 20 operators / 2.60M and 10 / 1.30M) | A (S), B (M4) | T22, T06 | S; M4 |
| **D5** | Reporter commit–reveal; refuted accusation costs ≥ r·S, capped; reward base = actual collected debit; consumed offence records it | §3.6 R-1…R-7 (commitments only on execution-proving keys; a DA default pays its earliest defaulted accuser); DA-6 (`min(⌈r·S_P(stage)⌉, min_collateral)`, held and refunded if the claim is convicted) | A (S, M3) | T39, T69, T75 | S; M3 |
| **D6** | t12-only Eq cap `min(C, k·G)` until Q1 is settled | §3.6 Eq; B-4 | A (S) | T76 | S |
| **D7** | 2M conservative until ADR-0153: RT#2 slash, no S0, `c_2M = 1` held to Final | SR-5 (C7 set), T-2(b) | A (S) | T20 | S |
| **F1** | Fake-root failures cannot be attributed (borrowed roots, garbage pin) | §3.10 J-1…J-7 as the audit's spec §4 (`PalwClaimStateV2.job_identity`, identity checks J1–J5, `IdentityMismatch = 9` … `LogitsNotStepOutput = 12`, `ExecutorRefuted = 4`); **v3.1: F1-M and F1c inside the gate** | A | T18, T18b–T18h, T18k, T18m, T18p, T-THREAD, T62 | M2 |
| **F2** | The execution-proving gate requires `job_id == claim_id`, which no block-lane claim can meet | §3.10 J-3 = the audit's spec §3 (`palw_offence_attribution`, `PanelFalseValidV2 = 3`, `palw_check_panel_false_valid_v2`, which binds by root and never compares `claim_id`) | A | T46a–T46n | M1 |
| **F3** | The DA court cannot attribute an offender that holds the data | = D3 | A | = D3 | M3 |
| **F4** | Honest S3-sampled Valids count toward the quorum | = D2 | B | = D2 | M4 |
| **F5** | P0-10 and withholding cost 0 today; the named filers do not exist | §3.9 and §4.2 per strategy (naive / garbage / borrowed); Phase 2 adds auto-PanelFalseValid, held StepLeaf demands, held dissections, the replay-mismatch builder (§7) | B (P2) | T18b, T18c, T54a–g, O-3 | P2; O |
| **F6** | m = 1 holds only for q ≥ q*; publish q | §4.3 (q* per class and door, at m = 3; q_min per class, since S3 sampling does not depend on the door) | B (text) | T06 | M4 |
| **F7** | The offender's Sybil can front-run the reporter reward | = D5 (R-3 commit–reveal); stated: R pays nothing against a rational offender, and precondition (i) no longer rests on R | A | T39, T75 | S |
| **F8** | X2's release counts Incapable as served | SR-1 cond. 2 (served = a carried `Valid` only; `Sampled` holds like `Incapable`) | A | T27, T68 | S; M3 |
| **F9** | A refuted accusation is ~4.15M× cheaper than the reward | DA-6 (refuted cost = r × the stage's reward base, capped at `min_collateral`) | A | T69 | M3 |
| **F10** | The reward base is not the collected collateral | R-2 (actual debit; 0 when `palw_bond_collateral_is_locked_v6` is false at the conviction DAA, read through a `#[borsh(skip)]` mirror of the withdrawal delay) | A | T39 (pre-drained, exited) | S |
| **F11** | p is set by whoever carries the licence first | SR-1b (a completing supplementary receipt by `min(L + 60, bound + window_receipt)` flips the release), carried by SR-10's V3 supplementary door | A | T74 | S |
| **F12** | The capacity table omits the redraw hold | T-2(d) capacity formula with f; SR-9 early redraw on ≥ 3 Unavailable | A | T08, T57 | S |
| **F13** | Eq zeroes a genesis bond | = D6; T76 "two genesis Eq slashes, licensing continues" | A | T76 | S |
| **F14** | Duty rule drifts above 1 for heavy classes | L-4: `duty_bind = min(max(λ, lock_2), ⌊(w+E)/seat_count⌋)`, bound ≤ 1 for every class | A | T77 | S |
| **F15** | X9 residual on 2M | L-4b: the 2M top-up is drawn-headroom-checked; the residual void is named in §4 | A | T78 | S |
| **F16** | The reward-inclusive invariant needs all k locks | R-6 states the premise j = k; T39's j < k 2M rows are expected-FAIL until ADR-0153 | A | T39 | S |
| **F17** | Reward timing and reorgs | R-4: sweep (step 2) → reward table → 3d; T07 reorg twin with different reporters | A | T07, T75 | S |
| **F18** | Trickle cost unnamed; FP licences do not tick the anchor | V-8 names the trickle cost; FP licences **do not** count (stated); S2 licences do not count either (C5) | A (text), O | T37, O-6 | S; O |
| **F19** | B-3 must follow duties out of `reserved_exposure` | B-3: gate on the duty/lock/claim part of `palw_bond_committed_v1 > 0`, open accuser exposure, plus unmatured rows; registration exposure is not in the gate (IMPL-8) | A | T23, T42 | S |
| **F20** | §6 overstates what v22 changes | §6 reworded; testnet-11 startup refusal; t11 rollback stays on `1f98d3bf`; T24 names two fences | A | T24, T80 | S |
| **F21** | After X1, `CourtExecutorGuilty` can bind only a zero root | V-5: bind only if `consumed.execution_root ≠ 0` and equal; dormant on t12 | A | T26 | S |
| **P2 A-KEY** | A raw `claim_id` beginning `0xFF` sorts among market rows | V-7: moved legs keyed under prefix `0x00` (`palw_vesting_payout_key_v1`, `palw_reporter_payout_key_v1`) | A | T47 | S |
| **P2 V-3** | The coinbase identity omits buyback and `panel_reserve_sompi` | V-3: the identity restated; notes for both | A (notes), B (T03) | T03 | S; P2 |
| **P2 round lane** | A convicted Final keeps its round-lane rights | V-2b: every conviction forfeits the claim's unminted rights, by root for execution-proving kinds and by claim (root 0) for the rest (v3.1, SPEC §4.5); the matured residual is named (§9.1 Q5: kept) | A | T81 | S |
| **P2 V-7 backlog** | 6-leg rows move one per block after a halt; the market is refused meanwhile | V-7: budget counted in new queue keys; 2 slots reserved for the market while it waits; ≥ 1 row per block guaranteed, mean 1.29 without market rows (model) | A | T82 | S |
| **Drill replay** | A drill chain's transactions must not replay onto public t12 | §8.2: salted genesis, drill-only keys, no card-key premine spend on any drill chain | B (P2-12) | T53 | P2 |
| **Panel room `e93be0f2` + `f8c91f19`** | The D5 room is a rate; a licence frees the replay; a court reopens it; utilization holds no class; **a C7 class (the 2M row) owes every claim until Final and is capped at `max_inflight_claims`** (post-edit 5: `f8c91f19` keyed this on `class_is_held_v1`, which also held the 8k row; the fix keys it on C7) | T-2(a), T-2(b) (the C7 hold), T-2(c) keeps an S2 licence's replay charged on classes outside C7 | A | T21, T20, T72 | S |

**The verifiers' findings on the v3 draft, as applied.** Rejections and partial adoptions are in "v3 review
disposition" at the end.

| id | what it found | closed by | owner | tests | phase |
|---|---|---|---|---|---|
| C1 | `Sampled` counted as served (contrary to D3) | SR-1 cond. 2, SR-1b, Q-1: `Sampled` holds to Final | A (S), B (Q-1) | T27, T68, T70 | S; M4 |
| C2 | The free-prompt abandon hold (audit C5) was silently dropped | SR-1 row "`Voided{BindTimeout}`, free prompt, on hold"; commitment takes `now_daa` | A | T02c | S |
| C3, V3S-05 | The free first redraw and the V1 door were missing from §4.3 | §4.3 rewritten per door and per filing assumption; Q-7/P2-6 pre-deadline filing makes the redraw non-free | B (text, P2-6), A (T06) | T06 | M4; P2 |
| C4 | DA-6 priced every stage as pre-licence | DA-6 priced per `PalwDaStageV1` on the stage's reward base (partial; see disposition) | A | T69 | M3 |
| C5, IMPL-17 | S2 licences tick the second clock | V-8, Q-5: the anchor settles only when `basis_k ≥ 2` | A (S), B (M4) | T37, T72 | S; M4 |
| C6 | M1 needs `job_anchor`, which was in M2 | **Superseded in v3.1:** the audit's F2 binds by root and needs no stored job identity (SPEC §3.8: no schema change), so J-1 (`job_identity`, SPEC §4.1–§4.2) is back in M2 | A | T-THREAD | M2 |
| C7 | A DA default charged honest partial seats for units outside their segment | DA-7, Q-6: S4 only on signers whose mask covers an unanswered unit; residual named (§4.2 #16) | A | T32, T66 | M3 |
| C8, IMPL-2(3) | Post-Final DA window unclear and internally inconsistent | DA-8 reconciled; DL-1 defines the retirement deferral; residual named (§4.2 #15) | A | T66, T40 | M3 |
| C9, V3S-10, IMPL-12 | Session caps and `StepLeaf` rules | DA-8: seats exempt from the lifetime cap, 4 sessions per seat per claim; DA-3: the named leaf is free, keyed by session; fused leaves go to a dissection | A | T65, T66, T18c | M3 |
| C10, V3S-13, IMPL-18(a) | Stale references; shard licensing was marked UNVERIFIED | §8.2 references; Q-3: `palw_shard_licensing` is dormant on t12 (verified) | B | T71 | M4 |
| C11 | Rules without owner or test | every rule now carries both; T84 (A-6), T54f/T54g (P2-8d/e) added | A, B | T84, T54f, T54g | S; P2 |
| C12 | Launch gate omitted Phase 2 tests | §8.3 item 5 | B | T03, T05, T23, T25, T47–T53, T58 | Gate |
| C13 | "2M: no S0" read as contradicting SR-5 | SR-5 sentence | A | T20 | S |
| V3S-01 | One silent partial seat could force an honest S2 claim into S0′ | Q-5: a first-panel `NotReplayBacked` redraws; upgrades count full-replay V2 `Valid`s from any seat; Q-7 node policy | B (M4), A (fold review) | T72b | M4 |
| V3S-02 | An open session on the head row froze every payout | DA-5: a row is re-keyed when a session opens; V-4(c) is then implied | A | T16, T82 | M3 |
| V3S-03 | Commit–reveal pays whoever predicts the key | R-3: `ReporterCommitted` refused on DA and court keys; R stated as no incentive against a rational offender | A | T39, T75 | S |
| V3S-04 | Post-Final convictions could be pushed past lock expiry | L-3/DA-5: locks follow the row's extended expiry; P2-6 accuses at licence when unserved | A (S, M3), B (P2-6) | T66 | M3; P2 |
| V3S-06 | Honest filers lose money on the garbage path | DA-6: refuted exposure is held and refunded if the claim is convicted; cost = r × reward base | A | T69 | M3 |
| V3S-07, IMPL-16 | Drawn event units fell outside the one-row attempt run | DA-3: rows drawn inside the committed run; held units for held classes; DA-4 `flat_answered` against the checker | A | T64 | M3 |
| V3S-08 | Pause credit cheapened room capture | DA-5: only panel-seat sessions pause a pre-Final claim | A | T67, T21 | M3 |
| V3S-09 | Stale base | header, T-2 and §6 re-anchored on `f8c91f19`; its reading that 8k is held to Final is **withdrawn by post-edit 5** (only C7 is held; T-2(b)) | A | T20, T21 | S |
| V3S-11 | Commitments pruned before their reveal | R-3: no commitment is pruned while a pending reward consumed after it is open | A | T39, T75 | S |
| V3S-12 | Post-Final DA bounties on silence | R-1/DA-7: a DA default's reward base is the producer's debit only; P2-7 retention is a normative duty | A, B (P2-7) | T66, T39 | M3; P2 |
| IMPL-1 | S2 upgrades and SR-1b cannot ride the V2-only supplementary door; masks stripped before locking | SR-10 (a V3 supplementary door); `lock_valid_receipts` takes masks | A (S), B (recount, M4) | T74, T72 | S; M4 |
| IMPL-2 | New deadline rules were not rebuildable | §3.13 DL-1: one pure deadline function in arm, rebuild, `expected_deadline` and `assert_deadline_consistency` | A | T40 | S; M3 |
| IMPL-3 | DA defaults and court convictions wrote no consumed offence | `PalwOffenceKindV1::DaDefault = 5`, `CourtConviction = 6` (v3.1: after the audit's 3 and 4); both forfeit round rights and reverse the Final | A | T81 | S; M3 |
| IMPL-4 | Accuser exposure had no ledger | A-6: `palw_accuser_exposure_v1`, free half, holds exit | A | T42, T69, T84 | S |
| IMPL-5 | R-2 needed a delay the fold does not have | `#[borsh(skip)] withdrawal_delay_daa` mirror; "already released" dropped | A | T39 | S |
| IMPL-6 | A plain params field moves every ruleset id | `#[borsh(skip)] rcore_plus_from_daa`; C7 hashed Some-only; Q-1/§6 reworded on encodings | A | T24 | S |
| IMPL-7 | New ML-DSA contexts need a V5 context set | §6: `PALW_V2_SIGNATURE_CONTEXTS_COMPLETE_V5` and its gates | A | T24 | S |
| IMPL-8 | Registration exposure would pin a registrant forever | B-3: not in the gate | A | T23 | S |
| IMPL-9 | The in-flight note could not see `basis_k` | T-2(c): the note reads `claim.rcore.basis_k` (v3.1: nested in `PalwClaimStateV2`, not a side record); every write of it re-notes | A | T21, T72 | S |
| IMPL-10 | V-7's throughput and T82 were wrong; the API contract moved | V-7 restated; T82 rewritten; §7.3 lists the contract changes | A, B | T82 | S; P2 |
| IMPL-11 | `Sampled` had no answer mapping | Q-1: `PalwSeatAnswerV2::Sampled`, which does not satisfy the ADR-0147 outsider | B (M4) | T70 | M4 |
| IMPL-13 | Header-less faces and header fallback could not carry the anchor; T60 had no crate | **Superseded in v3.1 by SPEC §4.2:** a missing header records `job_identity = 0` (never the `attempt_id` fallback, never a refusal: a refused own attempt fails the whole block); the real-claim tests live in `consensus/src/pipeline/virtual_processor/tests/t46_false_valid_real_claim.rs` | A | T46a–T46n, T-THREAD | M1; M2 |
| IMPL-14 | Missing indexes | §6 row 29 extended; the draw takes `now_daa` and depth | A | T40, T49 | S |
| IMPL-15 | G had no function | §3.6: G per claim; `palw_eq_cap_basis_v1` for Eq | A | T76 | S |
| IMPL-18(b–d) | T27/T74 not constructible; missing wire item | T27 on a non-floor class; T74 with an early licence; `PalwPanelFalseValidEvidenceV2` is the audit's (SPEC §3.2, M1), not an M4 item | A | T27, T74, T46b | S; M1 |

**What v3 keeps from v2 unchanged** (the review marked these closed): X1 (with F21's refinement), X3, X4, X5,
X6, X8, X11, X13, and the fence prerequisites of X15. X7 is kept with its residual named (§3.6). X10 is
**not** kept at launch (D1); its rule is kept as the target behind `palw_rcore_attributed_charging`.

---
