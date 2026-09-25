# 09 — Invariants

*Normative. The canonical list of the safety properties misaka-next must uphold. RFC 2119 keywords.
Every invariant here has exactly one test, named `inv_<area>_<nn>_<slug>`, that checks it; a chapter
that cites an invariant cites this list. Chapters 01–08 state each invariant where its rules live;
where a chapter and this file differ, this file governs and the difference is recorded in §4.*

## 0. How to read an entry

Each entry gives:

* **Statement**: a testable MUST / MUST NOT.
* **Why**: the reason, usually the attack it stops.
* **Stops**: attack names from [10-attack-model.md](10-attack-model.md). Each name is also a
  regression test.
* **next**: where misaka-next enforces the invariant (planned `crate/module::function`, see the crate
  map in [00-overview.md](00-overview.md) §7), and the **type** that makes a violation hard to write,
  if there is one.
* **t12**: the status at the frozen reference (`rcore/int-3` @ `a0af3c92`, [PROVENANCE.md](../../PROVENANCE.md)).
  The value is one of **holds**, **violated**, **partial**, **unknown**, or **n/a** (the property
  has no t12 counterpart). Evidence citations come from the owning chapter, whose author read them at
  `a0af3c92`. *(re-checked)* marks a citation the synthesis pass read again. `[unverified]` marks a
  claim nobody checked in code. *pending* marks a fix that exists only on a branch outside the
  reference.
* **Test**: the test name and its kind. **unit** covers compile-fail and static checks. **property**
  is randomised over inputs. **simulation** runs the adversarial simulator. **vector** is a fixed
  input/output pair shared across implementations.

Path shorthand: `core/X` means `consensus/core/src/X`, `pipeline/X` means `consensus/src/pipeline/X`,
and `processes/X` means `consensus/src/processes/X`. Other paths are repo-relative as written.

## 1. Index

| ID | Short name | Owner | t12 | Test |
| --- | --- | --- | --- | --- |
| **INV-CLAIM-01** | private DAA creates no tickets *(seed)* | 01, 03 | violated | `inv_claim_01_private_daa_does_not_create_tickets` |
| **INV-CLAIM-02** | unused capacity saturates at B *(seed)* | 03 | partial | `inv_claim_02_unused_capacity_saturates_at_b` |
| **INV-BOND-01** | splitting stake is neutral *(seed)* | 02 | partial | `inv_bond_01_split_does_not_raise_issuance_or_capacity` |
| **INV-FORK-01** | private DAA buys no comparative maturity *(seed)* | 06 | partial | `inv_fork_01_private_daa_does_not_buy_comparative_maturity` |
| **INV-POL-01** | unverified ≠ verified authority *(seed)* | 04 | violated | `inv_pol_01_unverified_commitment_lacks_verified_authority` |
| **INV-ECON-01** | extracted ≤ frozen *(seed)* | 08 | partial | `inv_econ_01_extracted_never_exceeds_frozen` |
| INV-TIME-01 | no wall clock in consensus | 01 | partial | `inv_time_01_no_consensus_function_reads_the_wall_clock` |
| INV-TIME-02 | time units do not mix | 01 | violated | `inv_time_02_time_units_do_not_mix` |
| INV-TIME-03 | SafeDaa only from licences | 01 | partial | `inv_time_03_safe_daa_advances_only_with_licences` |
| INV-TIME-04 | ChainFinalized ≤ Safe ≤ Local | 01 | n/a | `inv_time_04_finalized_le_safe_le_local` |
| INV-TIME-05 | node FinalizedDaa never decreases; chain one monotone | 01, 07 | unknown | `inv_time_05_finalized_daa_never_decreases` |
| INV-TIME-06 | heartbeats advance no safe clock; insertions bounded | 01 | violated | `inv_time_06_heartbeat_only_history_advances_no_safe_clock` |
| INV-TIME-07 | absence/release/refill read the safe clock | 01 | violated | `inv_time_07_absence_release_and_refill_rules_read_the_safe_clock` |
| INV-TIME-08 | no window shorter than its span | 01, synthesis | holds | `inv_time_08_no_window_is_shorter_than_its_span` |
| INV-DAA-01 | LocalDaa monotone, +1 max | 01 | holds | `inv_daa_01_local_daa_is_monotone_and_steps_by_at_most_one` |
| INV-DAA-02 | LocalDaa never outruns wall clock | 01 | partial | `inv_daa_02_local_daa_never_outruns_the_wall_clock` |
| INV-DAA-03 | non-ticking block cannot postpone | 01 | holds | `inv_daa_03_a_block_that_does_not_tick_does_not_postpone_the_next` |
| INV-DAA-04 | missed slots are lost | 01 | holds | `inv_daa_04_missed_slots_are_lost_not_banked` |
| INV-DAA-05 | one clock function | 01 | holds | `inv_daa_05_template_and_validation_compute_one_clock` |
| INV-DAA-06 | clock from header + parent only | 01 | holds | `inv_daa_06_clock_depends_only_on_header_and_selected_parent` |
| INV-CLAIM-03 | authority is a function of type | 03 | partial | `inv_claim_03_authority_is_a_function_of_type` |
| INV-CLAIM-04 | no header weight; unadmitted attempt confers nothing | 03 | violated | `inv_claim_04_unadmitted_attempt_has_no_weight` |
| INV-CLAIM-05 | escrow mints only from Final | 03 | holds | `inv_claim_05_escrow_mints_only_from_final` |
| ~~INV-CLAIM-06~~ | retired (§4) | — | — | — |
| INV-CLAIM-07 | unverified live weight bounded below verified | 03, 06 | partial | `inv_claim_07_unverified_live_weight_is_bounded` |
| INV-CLAIM-08 | Final only from Verified | 03 | holds | `inv_claim_08_optimistic_licence_never_finalizes` |
| INV-CLAIM-09 | claim deadlines on the safe clock | 03 | partial | `inv_claim_09_authority_deadlines_read_the_safe_clock` |
| INV-CLAIM-10 | claim life bounded | 03 | partial | `inv_claim_10_claim_life_is_bounded` |
| INV-POL-02 | every priced field pinned | 04 | holds | `inv_pol_02_every_priced_field_is_pinned` |
| INV-POL-03 | seeds not re-rollable | 04 | violated | `inv_pol_03_seeds_are_not_rerollable` |
| INV-POL-04 | pwu is the derivation | 04 | holds | `inv_pol_04_pwu_is_the_derivation` |
| INV-POL-05 | one execution, one claim | 04 | holds | `inv_pol_05_one_execution_one_claim` |
| INV-POL-06 | a Valid covers the named job | 04 | holds | `inv_pol_06_a_valid_covers_the_named_job` |
| INV-POL-07 | fake roots are convictable | 04 | partial | `inv_pol_07_fake_roots_are_attributable` |
| INV-POL-08 | target read at SafeDaa | 04 | violated | `inv_pol_08_target_reads_the_safe_context` |
| INV-PANEL-01 | Verified needs basis ≥ 2 and quorum | 05 | holds | `inv_panel_01_verified_requires_basis_two_and_quorum` |
| INV-PANEL-02 | header fields do not move the panel | 05 | violated | `inv_panel_02_anchor_nonce_and_timestamp_do_not_move_the_panel` |
| INV-PANEL-03 | executor never sits | 05 | holds | `inv_panel_03_executor_and_its_operator_never_sit` |
| INV-PANEL-04 | Valid and Unavailable quorums disjoint | 05 | holds | `inv_panel_04_valid_and_unavailable_quorums_are_disjoint` |
| INV-PANEL-05 | population predates the seed | 05 | holds | `inv_panel_05_population_predates_the_seed` |
| INV-PANEL-06 | outsider Valid required | 05 | holds | `inv_panel_06_outsider_valid_is_required` |
| INV-PANEL-07 | selection = acceptance predicate | 05 | holds | `inv_panel_07_selection_is_the_acceptance_predicate` |
| INV-PANEL-08 | basis-1 licence confers nothing | 05 | holds | `inv_panel_08_basis_one_licence_has_no_authority` |
| INV-PANEL-09 | Candidate class admits nothing | 05 | holds | `inv_panel_09_candidate_class_admits_nothing` |
| INV-PANEL-10 | node and fold share one court shape | 05 | violated | `inv_panel_10_node_and_fold_derive_one_court_shape` |
| INV-PANEL-11 | silence charges no seat | 05 | holds | `inv_panel_11_silence_charges_no_seat` |
| INV-PANEL-12 | panels stay live | synthesis | unknown | `inv_panel_12_panels_bind_from_genesis_and_after_a_halt` |
| INV-COURT-01 | unadjudicable mints no verdict | 05 | holds | `inv_court_01_unadjudicable_proof_mints_no_verdict` |
| INV-COURT-02 | default convicts only the defaulter | 05 | holds | `inv_court_02_default_is_never_a_signer_basis` |
| INV-COURT-03 | conviction rebuilds the root | 05 | holds | `inv_court_03_conviction_rebuilds_committed_root` |
| INV-COURT-04 | no session monopoly | 05 | violated | `inv_court_04_decoy_session_blocks_no_accusation` |
| INV-COURT-05 | forger's-race forfeit restored | 05 | violated | `inv_court_05_acquittal_on_forged_anchor_is_restored` |
| INV-COURT-06 | honest prosecution fits the window | 05 | partial | `inv_court_06_worst_honest_prosecution_fits_window` |
| INV-COURT-07 | reward needs a prior commitment | 05 | unknown | `inv_court_07_copied_reveal_earns_nothing` |
| INV-COURT-08 | a conviction conserves value | 05 | unknown | `inv_court_08_slashing_conserves_value` |
| INV-BOND-02 | seat distribution split-invariant | 02 | violated | `inv_bond_02_seat_distribution_is_split_invariant` |
| INV-BOND-03 | allowances superadditive | 02 | violated | `inv_bond_03_no_allowance_is_superlinear_in_accounts` |
| INV-BOND-04 | exit waits for every horizon | 02 | partial | `inv_bond_04_exit_waits_for_every_horizon` |
| INV-BOND-05 | vesting burnable, released on FinalizedDaa | 02 | partial | `inv_bond_05_private_daa_does_not_release_vesting` |
| INV-BOND-06 | one ledger, two ceilings | 02 | holds | `inv_bond_06_one_invariant_at_every_gate` |
| INV-BOND-07 | duty never exceeds forfeit | 02 | holds | `inv_bond_07_duty_never_exceeds_forfeit` |
| INV-BOND-08 | tier split-invariant | 02 | violated | `inv_bond_08_tier_is_split_invariant` |
| INV-BOND-09 | slashes burned and counted | 02 | holds | `inv_bond_09_slashes_are_burned_and_counted` |
| INV-ECON-02 | supply cap | 08 | partial | `inv_econ_02_supply_never_exceeds_the_cap` |
| INV-ECON-03 | every withheld sompi resolves once | 08 | partial | `inv_econ_03_every_withheld_sompi_resolves_once` |
| INV-ECON-04 | no silent burn | 08 | violated | `inv_econ_04_no_silent_burn` |
| INV-ECON-05 | heartbeats mint nothing | 08 | holds | `inv_econ_05_heartbeats_mint_nothing` |
| INV-ECON-06 | no leg leaves before the horizon | 08 | violated | `inv_econ_06_no_leg_leaves_before_the_horizon` |
| INV-ECON-07 | undetectable fraud EV-negative below s_target | 08 | partial | `inv_econ_07_undetectable_fraud_is_ev_negative_below_target_share` |
| INV-ECON-08 | self-report loses | 08 | holds | `inv_econ_08_self_report_loses` |
| INV-ECON-09 | one unit for reservation and weight | 08 | partial | `inv_econ_09_one_unit_for_reservation_and_weight` |
| INV-ECON-10 | unadmitted class takes no claim | 08 | partial | `inv_econ_10_unadmitted_class_takes_no_claim` |
| INV-FORK-02 | strict total order | 06 | holds | `inv_fork_02_comparator_is_a_strict_total_order` |
| INV-FORK-03 | selection path-independent | 06 | violated | `inv_fork_03_selection_is_path_independent` |
| INV-FORK-04 | independence of other candidates | 06 | partial | `inv_fork_04_no_candidate_can_move_the_order_of_two_others` |
| INV-FORK-05 | no weight without verified work | 06 | violated | `inv_fork_05_blocks_without_verified_work_add_no_weight` |
| INV-FORK-06 | every selection site agrees | 06 | violated | `inv_fork_06_every_selection_site_agrees` |
| INV-FORK-07 | anchor-less candidate never selected | 06 | partial | `inv_fork_07_no_candidate_below_the_finalized_anchor_is_selected` |
| INV-FORK-08 | maturing never lowers the key | 06 | holds | `inv_fork_08_maturing_never_lowers_the_key` |
| INV-FORK-09 | anchor advance preserves order | 06 | n/a | `inv_fork_09_anchor_advance_preserves_the_order` |
| INV-FINAL-01 | finalized anchor never reverts | 07 | partial | `inv_final_01_the_finalized_anchor_never_reverts` |
| INV-FINAL-02 | only selection advances finality | 07 | violated | `inv_final_02_only_selection_advances_finality` |
| INV-FINAL-03 | block anchor monotone and pure | 07 | partial | `inv_final_03_block_finalized_anchor_is_monotone_and_pure` |
| INV-FINAL-04 | nothing unresolved below own claim frontier | 07 | partial | `inv_final_04_nothing_unresolved_below_the_anchors_own_frontier` |
| INV-FINAL-05 | empty blocks do not finalize | 07 | violated | `inv_final_05_empty_blocks_do_not_finalize` |
| INV-FINAL-06 | pruning never passes the anchor | 07 | partial | `inv_final_06_pruning_never_passes_the_anchor` |
| INV-FINAL-07 | conflicting finalization costs more than it pays | 07 | unknown | `inv_final_07_a_conflicting_finalization_costs_more_than_it_pays` |
| INV-BLK-01 | the block transition is one pure ordered function | 11 | partial | `inv_blk_01_block_transition_is_one_pure_ordered_function` |
| INV-BLK-02 | refused attempts confer nothing | 11 | partial | `inv_blk_02_refused_attempts_tick_nothing_and_are_paid_nothing` |

Totals: 88 live invariants (one retired). At the reference, 31 hold, 22 are violated, 28 are partial,
5 are unknown and 2 have no counterpart.

## 2. Seed invariants

### INV-CLAIM-01 — private DAA advancement creates no tickets *(seed)*

* **Statement.** Private DAA advancement MUST NOT increase lottery eligibility beyond a stated
  bound. At every block, the ticket target in force, the number of claims the block may admit, and
  every refill of admission capacity MUST be functions of the block's `SafeDaa` (and state), never of
  its `LocalDaa`. Compare a branch `A'` with a branch `A` that carries the same verified content. At
  corresponding blocks, `SafeDaa(A') − SafeDaa(A)` MUST be at most the slots `A` left unclaimed plus
  one (01 S5 with INV-DAA-02). So `A'`'s extra admissions are at most `rate` times that, capped by
  `B` per refill.
* **Why.** A branch that ticks its local clock with heartbeats and produces little must not ease its
  own lottery or refill its own capacity. Heartbeats cost only hashes (01 §2.2). `SafeDaa` copies
  `LocalDaa`'s spacing (01 §2.3), so an equality would be false. The bound is what INV-DAA-02 makes
  true, and honest heartbeat coverage of every slot (10 §2.2 H6) keeps it at one tick.
  *[Synthesis edit, review: the draft claimed equality through a property S4 that is false for
  insertions.]*
* **Stops.** `private_work_target_easing`, `heartbeat_clock_acceleration`,
  `w_controller_counts_nonfinal_blocks` (epoch half), `idle_class_target_relaxation`.
* **next.** `consensus/claims::ClaimBucket::refill(now: SafeDaa)`;
  `pol/lottery::target_in_force(at: SafeDaa)`; `pol/lottery::step_work_target(.., &ClosedEpoch {
  index: SafeEpoch, .. })`. **Type:** `SafeDaa` = `Daa<Safe>`, so `refill(LocalDaa)` and a
  `LocalDaa` epoch do not compile (01 §2.6). Rules: DAA-R17, CLAIM-R10, POL-R6.
* **t12.** **violated (bounded).** `W` steps once per `LocalDaa` epoch and eases by ÷4 per empty
  epoch, down to `W₀` (`core/palw_state_v2.rs:22736-22775`, `core/palw_work_target_v1.rs:98-111`).
  The pooled receipt target eases on a silent epoch (`core/palw_state_v2.rs:23117-23121`). The
  ADR-0123 release grows with `DAA mod L` (`:22497-22528`), but it is unreachable on t12 for model
  classes (03 §6.3).
* **Test.** `inv_claim_01_private_daa_does_not_create_tickets`, property plus simulation. Two branches
  share the same verified content. The baseline claims every slot (H6). The other is raced by
  heartbeats stamped at `now + DRIFT_MS`. The test asserts `SafeDaa` differs by at most one tick at
  every block, targets are equal except within one tick of a `SafeDaa` epoch boundary, and admissible
  counts differ by at most `⌈rate⌉`. A second case leaves `m` slots unclaimed on the baseline and
  asserts a gap of at most `m + 1` ticks.

### INV-CLAIM-02 — unused issuance capacity does not accumulate beyond B *(seed)*

* **Statement.** Unused issuance capacity MUST NOT accumulate beyond `B`. For every lane bucket,
  `tokens ≤ B` holds at every block, refill saturates at `B`, and refill is path-independent:
  `refill(a).refill(b) == refill(b)` for `a ≤ b`. No other admission control MAY bank silence. A
  target eased by silence counts as banked capacity.
* **Why.** A burst after a quiet stretch, public or private, must be bounded by `B`, however long the
  stretch lasted.
* **Stops.** `private_work_target_easing` (the burst half), `idle_class_target_relaxation`,
  `junk_claim_composite_stall`.
* **next.** `consensus/claims::ClaimBucket { refill, try_take }`, with a private milli-token field.
  Won free-prompt quanta lapse (CLAIM-R14). Rules: CLAIM-R10, DAA-R17.
* **t12.** **partial.** Epoch budgets do not carry over (`core/palw_admission_v2.rs:524-575`) but are
  not read for model classes on t12 (`:515`). Won free-prompt quanta lapse
  (`core/palw_freeprompt_v3.rs:981-987`). The `W` controller banks silence as an easier target, down
  to `W₀` (`core/palw_work_target_v1.rs:98-111`).
* **Test.** `inv_claim_02_unused_capacity_saturates_at_b`, property.

### INV-BOND-01 — splitting stake is neutral *(seed)*

* **Statement.** Splitting one stake into N accounts MUST NOT increase the aggregate steady-state
  issuance rate those accounts can earn, nor their aggregate concurrent claim capacity.
* **Why.** Identities are free, so any per-account term can be multiplied. Issuance and capacity are
  the first things an attacker would multiply.
* **Stops.** `bond_split_amplification`.
* **next.** `consensus/bonds::work_room` is linear and rounds down (BOND-R2). There is no per-account
  count cap (CLAIM-R6). The lane bucket is network-wide, never per account (03 §2.6). No type
  enforces this; a property over random partitions does.
* **t12.** **partial.** Issuance is split-neutral: the escrow is carved from the producing block's own
  subsidy (`core/palw_state_v2.rs:28456-28460`), budgets are per class (`:4317-4325`), the ceiling
  rounds down (`:2609`), and a ticket is a function of the execution (`core/palw_attempt_v2.rs:581-589`).
  Capacity is not split-neutral: the per-bond class share `⌈c_class/2⌉` is a count
  (`core/palw_state_v2.rs:12239`, *re-checked*). Chapter 02 reported "holds" for the issuance half
  alone.
* **Test.** `inv_bond_01_split_does_not_raise_issuance_or_capacity`, property: random partitions of
  S into N accounts, for N from 1 to 200, compared with the unsplit account.

### INV-FORK-01 — private DAA buys no comparative maturity *(seed)*

* **Statement.** A branch MUST NOT gain comparative maturity solely because its private DAA is ahead
  of the competing branch. Formally, let `A'` equal `A` with the same verified content (the same
  verified entries, dispute openings and proven verdicts in the same chain order) but a faster
  `LocalDaa`. Then `safe(A') ≤ safe(A)` and `live(A') ≤ live(A)` MUST hold at corresponding blocks.
* **Why.** If maturity is time on a branch's own clock, the fastest clock wins. Safety is burial by
  verified weight with no unreleased dispute, and no clock enters it (06 §3.3). `Final`, which elapses
  on `SafeDaa`, does not enter the keys. The only clock-driven input left is voids, and a faster clock
  can only add voids. *[Synthesis edit, review: the draft keyed `safe` on `Final` claims. Under a
  `SafeDaa` challenge window burial did not imply settlement, and the race-proof argument failed.]*
* **Stops.** `private_daa_finality_acceleration`, `heartbeat_padding_buys_frontier_key`,
  `pairwise_context_cycle`, `junk_candidate_context_drag`, `stalled_leader_context_drag`.
* **next.** `consensus/fork_choice::fork_key(view: &Admissible, ctx: &SafeContext, ..)`. No argument
  may be a `Daa<_>`, a `BlueScore` or a `Timestamp` (FORK-R8). `VerifiedEntry` has no field for
  `Final` status. **Types:** `Weight` has no constructor from a header or a clock, and
  `SafeContext { anchor }` carries no clock reading. Rules: FORK-R3, FORK-R4, FORK-R16, DAA-R14.
* **t12.** **partial.** The DAA is paced to the wall clock once the floor is armed
  (`processes/difficulty.rs:459-517`; `pipeline/header_processor/pre_pow_validation.rs:82-97`).
  But claim `Final` is reached on the branch's own DAA (`core/palw_state_v2.rs:3233-3243`, `:23666`,
  `:23771`, *re-checked*), and comparator key 1 is the blue score of the deepest `Final`
  (`core/palw_fork_choice.rs:72-78`, *re-checked*; `core/palw_state_v2.rs:20935`).
* **Test.** `inv_fork_01_private_daa_does_not_buy_comparative_maturity`, property plus simulation.
  Generate `A` and a clock-raced `A'` with identical verified content, including open disputes and
  verdicts. Assert `(safe, live)(A') ≤ (safe, live)(A)` at every corresponding block. The real-time
  meaning of `d_bury` (W1, FORK-R16) is tested by the same simulation.

### INV-POL-01 — unverified commitments lack verified authority *(seed)*

* **Statement.** An unverified LLM commitment MUST NOT obtain the same consensus authority as
  verified computation. Concretely, a `LotteryWin` or `UnverifiedClaim` carries none of A3 (safe
  weight), A4 (safe clock), A7 (seed), A8 (controller input) or A10 (rights) from 03 §2.5. It carries
  no mint and no header work (no block carries any). Its live weight is strictly less than that of a
  `VerifiedClaim` of equal work, and zero by default (06 FORK-R7).
* **Why.** A lottery win can be fabricated without running the model, at about seven BLAKE2b calls
  per try (04 §2.4).
* **Stops.** `private_fake_root_burst`, `unverified_live_weight`, `failed_lottery_blue_weight`,
  `panel_draw_seed_grind`, `w_controller_counts_nonfinal_blocks`, `open_claim_frontier_pin`.
* **next.** `consensus/claims/authority.rs`: each authority function takes the type that carries the
  authority (`safe_term(&FinalClaim)`, `seed_leaf(&FinalClaim)`, `live_term(LiveRef)`).
  `consensus/fork_choice::ChainView` has no field for unverified claims. `pol/lottery::LotteryWin`
  exposes no `Weight`. **Types:** the typestate itself.
* **t12.** **violated.** It holds for safe weight and for the mint (`processes/coinbase.rs:256-268`).
  It is violated for block work, since every attempt header earns 2²⁰
  (`processes/ghostdag/protocol.rs:666-671`, *re-checked*). It is violated for live weight
  (`core/palw_state_v2.rs:28436-28444`), for seeds (`pipeline/virtual_processor/processor.rs:10021-10045`,
  `core/palw_panel_v2.rs:1480-1510`, *re-checked*) and for controller input
  (`core/palw_state_v2.rs:28543-28558`, *re-checked*). Chapter 05 wrote "partial" (holds in the
  letter). The canonical status is violated, because `Provisional` and a counted licence carry equal
  live weight on t12 (03 §2.5).
* **Test.** `inv_pol_01_unverified_commitment_lacks_verified_authority`, unit (a compile-fail suite
  over authority functions) plus property (fork keys with and without a fabricated claim are equal).

### INV-ECON-01 — extracted never exceeds frozen *(seed)*

* **Statement.** For every claim of an admitted class, at every point before its conviction horizon
  closes, `Extracted(claim) ≤ Frozen(claim)` MUST hold. `Frozen` counts only unpaid escrow, the
  reserved commitment, the live locks of the claim's signers and its unreleased vesting row.
  `Extracted` counts every paid leg, executed buyback, spent right and the priced value of its
  weight. This refines the seed statement, "maximum guaranteed attacker gain before detection MUST
  NOT exceed guaranteed slashable collateral".
* **Why.** This is the operator's design bar: the value an attacker can definitely take out must not
  exceed the value the chain can definitely recover, counting only value consensus has frozen
  (08 ECON-R9).
* **Stops.** `private_fake_root_burst` (economic half), `colluding_quorum`, `early_extraction`,
  `weight_unit_gap`, `unattributable_2m_claims`, `held_attention_lie_unattributable`,
  `seat_reward_exceeds_slash`, `withholding_producer_uncharged`.
* **next.** `consensus/bonds::econ::{frozen, extracted, econ_margin, owner_margin, admit_class_econ}`.
  **Type:** `Frozen` can be constructed only from its four sources, and there is no constructor from
  free stake (08 §2.4). Rules: ECON-R7, ECON-R8, ECON-R9, ECON-R11, ECON-R12; BOND-R5, BOND-R8,
  BOND-R12.
* **t12.** **partial.** The floor class holds given detection (+3,520.94 MSK per claim). The 8k class
  fails above about 6.63M MSK of Sybil stake, because the `AttnFused` lie has no conviction route
  (A-held is *pending*). The 2M class fails (`core/palw_work_target_v1.rs:217-228`). The figures are
  ADR-0152 §4.4's, not re-derived.
* **Test.** `inv_econ_01_extracted_never_exceeds_frozen`, property (every stage of every admitted
  class) plus simulation (colluding quorum, early-extraction schedule). Chapter 05's variant name
  `inv_econ_01_colluding_quorum_locks_exceed_gain` becomes one case of this test.

## 3. Invariants by area

### 3.1 TIME — clock types and authority

**INV-TIME-01 — no consensus function reads the wall clock.**
The future-drift check MUST defer admission and MUST NOT mark a header invalid.
*Why:* purity, and a header that becomes valid later must not be refused forever.
*Stops:* `heartbeat_future_stamp_step` (in part), `clock_reference_node_local`.
*next:* the consensus crates depend on nothing that yields wall time. `admit_timestamp` lives in
node code (DAA-R6), and a crate-graph test enforces the split.
*t12:* **partial**. `unix_now()` is read inside header validation
(`pipeline/header_processor/pre_ghostdag_validation.rs:434-440`); whether the refusal is cached as
invalid is [unverified].
*Test:* `inv_time_01_no_consensus_function_reads_the_wall_clock`, unit (static and crate-graph).

**INV-TIME-02 — values of different time units cannot be compared, added or assigned.**
*Why:* t12 set blue-score depths from DAA windows. *Stops:* `blue_depth_unit_mismatch`,
`span_unit_mismatch_short_windows`, `inactivity_leak_window_mismatch`.
*next:* newtypes in `primitives` (`Timestamp`, `SlotIndex`, `DaaSpan`, `BlueScore`) and `Daa<C>` in
`consensus/daa`, with no cross-unit operators and no `From` impls. **Type:** `Daa<C: ClockKind>`.
*t12:* **violated**: `finality_depth` (blue score) = `window_challenge` (DAA)/2
(`core/config/params.rs:2781`, *re-checked*), and the pruning depth (blue score) ≥ the DAA claim
lattice (`:2697-2718`).
*Test:* `inv_time_02_time_units_do_not_mix`, unit (compile-fail).

**INV-TIME-03 — `SafeDaa` is minted only by `safe_daa`.**
Moving it past `x` MUST need `D_SAFE` licences recorded at `LocalDaa ≥ x` on that chain, or chain
finality past `x`.
*Why:* one definition of "safe". A private safe clock costs `D_SAFE` licences. On a branch the
adversary produces, those come from its own seats (`P_cap`, 10 §2.1 C11).
*Stops:* `private_daa_finality_acceleration`, `second_clock_heartbeat_escape`.
*next:* `consensus/daa::safe_daa(parent_ring, depth, floor: ChainFinalizedDaa)`. **Type:** `Daa<Safe>`
has a private field and exactly one mint.
*t12:* **partial**. The second-clock precursor ticks at licences
(`core/palw_state_v2.rs:18930-18940`) but escapes to DAA only (`:2224-2229`, *re-checked*), and
neither `Final` nor the safe frontier uses it (`:23766-23772`, `:20919-20946`).
*Test:* `inv_time_03_safe_daa_advances_only_with_licences`, property.

**INV-TIME-04 — `ChainFinalizedDaa ≤ SafeDaa ≤ LocalDaa` at every block.**
*Why:* the ordering makes the conservative direction explicit, and it is the premise of
INV-TIME-08. It is stated for the chain-relative anchor: the node's `FinalizedDaa` is not a chain
clock, and a block below a node's anchor would otherwise get `SafeDaa > LocalDaa`. *next:* `safe_daa`
property S2 (01 §4). *t12:* **n/a** (one untyped score).
*Test:* `inv_time_04_finalized_le_safe_le_local`, property.

**INV-TIME-05 — a node's `FinalizedDaa` never decreases, across every reorg; `ChainFinalizedDaa` is
non-decreasing along every chain.**
The first half is a corollary of INV-FINAL-01 on the node type. The second is 07 FINAL-R6 on the chain
type, which reverts with its branch. No state transition reads the node type (01 DAA-R10).
*Stops:* `long_range_rewrite`.
*next:* `consensus/finality::advance_finality` and `chain_finalized_daa`; `FinalizedDaa`'s only
constructor is `FinalizedAnchor::daa`, `ChainFinalizedDaa`'s is `chain_finalized_daa` (07 §2.2).
*t12:* **unknown**. The finality point is set by blue depth (`processes/block_depth.rs:59-80`);
node-side refusal was not read.
*Test:* `inv_time_05_finalized_daa_never_decreases`, simulation.

**INV-TIME-06 — heartbeat-only history advances neither `SafeDaa` nor `ChainFinalizedDaa`; inserted
ticks move later `SafeDaa` by at most their number.**
Appending blocks that perform no licence and carry no verified weight leaves both unchanged (S4).
Inserting `k` ticking blocks before a licence raises every later `SafeDaa` by at most `k` (S5).
*Why:* a heartbeat costs 2²⁴ hashes, and a private branch pays that freely. *[Synthesis edit, review:
the draft claimed invariance under any insertion (S4), which is false. `SafeDaa` is the `LocalDaa`
of a licence-recording block.]*
*Stops:* `heartbeat_clock_acceleration`, `second_clock_heartbeat_escape`,
`private_daa_finality_acceleration`.
*next:* properties S4 and S5 of `safe_daa` (01 §4). The licence ring's only input is the `verify`
transition, recorded at the performing chain block's `LocalDaa` (DAA-R12). FINAL-R7.
*t12:* **violated**. The second clock escapes after `2 × window_court` without a licence
(`core/palw_state_v2.rs:2224-2229`; `core/palw_panel_var_v1.rs:241-243`, *re-checked*), and `Final`
is reached by the DAA sweep alone (`core/palw_state_v2.rs:19338-19342`, `:23766-23772`).
*Test:* `inv_time_06_heartbeat_only_history_advances_no_safe_clock`, simulation. Append heartbeat-only
stretches and assert both clocks unchanged. Then insert `k` ticks before a licence and assert the
`SafeDaa` gap is at most `k`.

**INV-TIME-07 — every rule that punishes absence, grants or releases, or refills eligibility reads
`SafeDaa` or `ChainFinalizedDaa` as 01 §2.7 assigns; no state transition reads the node's
`FinalizedDaa`.**
*Why:* absence on a private branch is not evidence of absence, and release and refills are exactly
what an attacker would buy with a fast clock.
*Stops:* `private_absence_conviction`, `second_clock_heartbeat_escape`, `private_work_target_easing`,
`economic_deadline_on_heartbeat_clock`.
*next:* 01 §2.7's authority table, applied through the deadline types: `Deadline<Safe>` and
`Deadline<ChainFinal>` fields in 03 §2.3, 02 §2.1 and 05 PANEL-R21. A census test enumerates every
function that takes a `Daa<_>` and checks the kind against the table, and a crate-graph test checks
that no state crate imports `FinalizedDaa`.
*t12:* **violated**. See census rows C24, C26, C29–C38 and C52–C56 in 01 §6.2, for example
`core/palw_state_v2.rs:23676-23743` (receipt timeout), `:21933-21960` (court), `:22736-22775`
(`W`), and `core/dns_bft_v1.rs:381-388` (leak).
*Test:* `inv_time_07_absence_release_and_refill_rules_read_the_safe_clock`, unit (census) plus
simulation.

**INV-TIME-08 — no window is shorter, on its own branch's clock, than its span.** *(added in
synthesis)*
A `Deadline<C>` marked at block `B` with span `s` MUST NOT be elapsed at any descendant `D` with
`LocalDaa(D) ≤ LocalDaa(B) + s`.
*Why:* the deadline's mark must be the creating block's `LocalDaa`. If a window judged on `SafeDaa`
were measured from the `SafeDaa` at its creation, a safe clock that lags by `L > s` would close the
window almost as soon as it opened (`safe_mark_window_collapse`). This invariant fixes the
disagreement between the 01 §2.6 draft (a `LocalDaa` mark) and the 03 §2.2 draft (a mark at
`accepted_safe`).
*Stops:* `safe_mark_window_collapse`.
*next:* `consensus/daa::Deadline::<C>::after(mark: LocalDaa, span)` is the only constructor; no
`Daa<C> + DaaSpan` exists for any `C` (compile-fail test), and `elapsed(now: Daa<C>)` uses
`ChainFinal ≤ Safe ≤ Local`. **Type:** `Deadline<C>`.
*t12:* **holds**. Every deadline is the claim's own DAA plus a window
(`core/palw_state_v2.rs:28540`), swept at `deadline < daa` (`:23665-23667`, *re-checked*).
*Test:* `inv_time_08_no_window_is_shorter_than_its_span`, property.

### 3.2 DAA — the local slot clock

**INV-DAA-01 — `LocalDaa` is non-decreasing along every selected chain and rises by at most one per
block.**
*Why:* a clock that can jump lets one block expire many deadlines. *Stops:*
`unpriced_lane_advances_daa`, `heartbeat_width_burst`. *next:* `consensus/daa::clock_step`, P1.
*t12:* **holds** (`processes/difficulty.rs:44-56`, `:459-517`).
*Test:* `inv_daa_01_local_daa_is_monotone_and_steps_by_at_most_one`, property.

**INV-DAA-02 — every header satisfies `daa_score ≤ clock_slot`.**
It follows that an admitted chain's `LocalDaa` never exceeds `(now + DRIFT_MS − genesis_ts) / SLOT_MS`.
*Why:* windows sized in DAA assume the DAA does not outrun real time (`core/palw_court_deadline.rs:191`).
*Stops:* `heartbeat_clock_acceleration`, `heartbeat_future_stamp_step`,
`clock_reference_window_escape`.
*next:* `check_header_clock`. The header commits `clock_slot`, and `validate_params` refuses
`DRIFT_MS ≥ SLOT_MS`.
*t12:* **partial**. H3 and H5 (`pipeline/header_processor/pre_pow_validation.rs:82-97`) bound the rate
to one tick per 120 s plus a lead of at most 2 slots. The bound is void when the reference falls out
of the 264-blue window (`core/palw_clock_cursor_v1.rs:118-122`, `:195-224`;
`processes/difficulty.rs:502-505`).
*Test:* `inv_daa_02_local_daa_never_outruns_the_wall_clock`, property.

**INV-DAA-03 — a block that does not tick leaves the clock unchanged and cannot postpone the next
tick.**
*Stops:* `clock_slot_rule_freeze`. *next:* `clock_step`, P3.
*t12:* **holds** (`core/palw_clock_cursor_v1.rs:103-122`; `processes/difficulty.rs:476-505`).
*Test:* `inv_daa_03_a_block_that_does_not_tick_does_not_postpone_the_next`, property.

**INV-DAA-04 — missed slots are lost: one tick per block, however many slots it skipped.**
*Why:* a banked backlog lets the clock run fast after an outage (ADR-0142 §4).
*next:* `clock_step`, P4. *t12:* **holds** (`processes/difficulty.rs:502-517`).
*Test:* `inv_daa_04_missed_slots_are_lost_not_banked`, property.

**INV-DAA-05 — template construction and header validation compute the clock with one function.**
*Why:* the two drifted apart twice in t12 (ADR-0142 §5). *next:* both call `clock_step`, and a test
walks every caller. *t12:* **holds** (`pipeline/header_processor/pre_pow_validation.rs:81-97`;
`processes/difficulty.rs:393-400`), not re-tested.
*Test:* `inv_daa_05_template_and_validation_compute_one_clock`, vector.

**INV-DAA-06 — a block's clock is a function of its header and its selected parent's header only.**
Pruned, snapshot and archival nodes MUST agree.
*Stops:* `clock_reference_node_local`. *next:* the header-committed `ClockState`, with no window
lookup. *t12:* **holds** (`processes/difficulty.rs:470-500`), but the result depends on how far the
window reaches (C05).
*Test:* `inv_daa_06_clock_depends_only_on_header_and_selected_parent`, vector (archival and pruned
replays).

### 3.3 CLAIM — typestate, authority, capacity

**INV-CLAIM-03 — a claim exercises exactly the authorities its type carries in 03 §2.5.**
A `VoidedClaim` or `ConvictedClaim` MUST acquire no new authority.
*Why:* authority is a type, so exercising an authority the type lacks is a compile error.
*Stops:* every INV-POL-01 attack, and `conviction_leaves_final_rights`.
*next:* `consensus/claims/authority.rs`, with private fields and one transition function per
typestate (CLAIM-R1). *t12:* **partial** (`processes/ghostdag/protocol.rs:666-671`;
`pipeline/virtual_processor/processor.rs:10021-10045`; `core/palw_state_v2.rs:28543-28558`).
*Test:* `inv_claim_03_authority_is_a_function_of_type`, unit (compile-fail) plus property.

**INV-CLAIM-04 — no block acquires any fork-choice or ordering weight from its header, and an
attempt that is not admitted confers nothing.**
A lost-lottery attempt header is invalid (11 BLK-R2). A refused own attempt makes its block
ineligible for the selected chain. A refused merged attempt is skipped. Neither ticks the clock nor
is paid. *(Sharpened twice. The 03 draft said "fork-choice weight above ε"; synthesis made it
"block-level work above ε". Review: `ε` had no consumer once blue work was dropped, so it is gone.)*
*Stops:* `failed_lottery_blue_weight`, `bondless_attempt_row_grind`.
*next:* `consensus/validation` checks the ticket against the carried target at the header stage and
the carried target against `target_in_force(SafeDaa)` at the state stage (11 BLK-R2, BLK-R5).
`consensus/fork_choice` has no `Weight` constructor from headers (FORK-R9, CLAIM-R3, POL-R5).
*t12:* **violated** (`processes/ghostdag/protocol.rs:666-671`, `consensus/pow/src/lib.rs:591-596`,
*re-checked*; `pipeline/virtual_processor/processor.rs:11630-11652`).
*Test:* `inv_claim_04_unadmitted_attempt_has_no_weight`, property: a lost ticket is refused at the
header stage; a won but refused attempt leaves every key, the clock and the coinbase as if it were
absent.

**INV-CLAIM-05 — escrow is minted only via a matured `FinalClaim` vesting row.**
A voided or convicted claim's escrow MUST never be minted.
*Stops:* `merged_work_payout_mismatch`, `algo4_credit_mint_holes`, `conviction_leaves_final_rights`.
*next:* `vesting_row(&FinalClaim)`, released only by `vesting_releasable(.., ChainFinalizedDaa, ..)`
(BOND-R12, CLAIM-R11).
*t12:* **holds** (`processes/coinbase.rs:256-273`; `core/palw_state_v2.rs:19212`, `:19323`,
`:19351-19400`).
*Test:* `inv_claim_05_escrow_mints_only_from_final`, property.

**INV-CLAIM-07 — unverified live weight is bounded below verified live weight.**
An `UnverifiedClaim` MUST add strictly less live weight than a `VerifiedClaim` of equal work (zero by
default). `live − safe ≤ β_v·(non-voided verified weight not counted in safe: unburied, or held by
an unreleased dispute) + β_u·(unverified weight)` MUST hold, and burial MUST never lower `live`.
*(Review: the draft bounded it by "unsettled verified weight", which is false under 06's formula
whenever a settled entry is not yet buried.)* *(Reconciled: the 03 draft required `β < 1` for every claim, while
06 FORK-R16 allows `0 < β ≤ 1` for verified work and gives unverified work zero.)*
*Stops:* `unverified_live_weight`, `private_fake_root_burst`, `fresh_tip_unresolved_fallback` (the
monotonicity half).
*next:* `consensus/fork_choice::fork_weights` (FORK-R7) and `consensus/claims::chain_weights`
(CLAIM-R4). **Type:** `LiveRef`.
*t12:* **partial**. The contribution is bounded and monotone (`core/palw_fork_choice.rs:56-64`,
*re-checked*; β = 100‰, `core/palw_fp_devnet_v3.rs:34`), but `Provisional` and a counted licence are
priced identically.
*Test:* `inv_claim_07_unverified_live_weight_is_bounded`, property.

**INV-CLAIM-08 — a `FinalClaim` is constructed only from a `VerifiedClaim`.**
No stage of `UnverifiedClaim`, including `Optimistic`, reaches `Final`.
*Stops:* `optimistic_single_seat_licence`, `sampled_verification_evasion`.
*next:* `mature(c: VerifiedClaim, ..)` (CLAIM-R8). **Type:** the signature.
*t12:* **holds** past `palw_rcore_plus`: the recount (`core/palw_state_v2.rs:2920-2924`) and the Final
gate, which redraws and then voids `NotReplayBacked` a licence awaiting replay and finalizes only the
rest (`:23744-23771`, *re-checked*). (`:2930-2933` is the panel-room counting predicate, not the gate.)
*Test:* `inv_claim_08_optimistic_licence_never_finalizes`, unit plus property.

**INV-CLAIM-09 — every claim transition that grants authority or charges collateral elapses on a
`Deadline<Safe>`, and every value release on a `Deadline<ChainFinal>`.**
Only uncharged removals (`NoRing`) use a `Deadline<Local>`. This is the claims instance of
INV-TIME-07.
*Stops:* `private_daa_finality_acceleration`, `private_absence_conviction`,
`heartbeat_clock_acceleration`.
*next:* the typed stage fields of 03 §2.3 (CLAIM-R9).
*t12:* **partial**. Locks use two clocks (`core/palw_panel_var_v1.rs:202-245`), while `Final`, the
timeouts and the `W` steps read `LocalDaa` (`core/palw_state_v2.rs:23662-23771`, `:22736-22775`).
*Test:* `inv_claim_09_authority_deadlines_read_the_safe_clock`, property.

**INV-CLAIM-10 — every claim is resolved by `Deadline::<Safe>::after(accepted_at, D_max +
W_conviction)`, or voided uncharged earlier.**
Resolved means terminal, or `Final` with `conviction_ends` elapsed. Pruning never passes an
unresolved claim (INV-FINAL-04, INV-FINAL-06), so no comparison with a pruning depth in another unit
is made. *(Review: the draft compared `D_max` with the pruning horizon, which next measures in safe
anchors.)*
*Stops:* `open_claim_frontier_pin`, `span_unit_mismatch_short_windows`.
*next:* `validate_params` checks the lattice windows against `D_max` and `W_conviction`, all
`DaaSpan` (CLAIM-R13).
*t12:* **partial**. A lattice-versus-horizon test exists (named at `core/palw_state_v2.rs:23695`, its
body [unverified]); class-derived `D(c)` is *pending* (`feat/t12-class-verify-deadline`).
*Test:* `inv_claim_10_claim_life_is_bounded`, property plus simulation.

### 3.4 POL — lottery, commitment, seeds

**INV-POL-02 — the ticket is a function of (execution commitment, derived anchor) only.**
Every priced field MUST be chain-equal, derived, replay-checked, or the position field.
*Stops:* `unpinned_priced_field_free_draw`, `nonce_free_lottery_draws`, `sibling_identity_pow_reuse`.
*next:* `pol/commitment::execution_commitment` hashes the whole body, and an exhaustive field
classification fails the build when a new field is unclassified (POL-R1, POL-R3).
*t12:* **holds** (`core/palw_attempt_v2.rs:562-589`; pins at `core/palw_admission_v2.rs:833-850`;
classification test at `core/palw_attempt_v2.rs:1103-1107`).
*Test:* `inv_pol_02_every_priced_field_is_pinned`, unit plus property.

**INV-POL-03 — no seed consumed by consensus can be re-rolled by one party for less than one
admitted, licensed claim per try.**
No seed may read a block hash, a claim id or an unverified commitment. Its only per-claim input is
the chain-assigned admission index (04 POL-R9). On the honest chain a licence needs honest seats. On
a branch the adversary produces, it needs only its own seats, so there the price is a bucket token
and a `P_cap`-probability licence, not an inference (10 §2.1 C11). *(Review: the draft said "one
verified execution per try", which is false once a private branch self-licenses fake roots.)*
*Stops:* `panel_draw_seed_grind`, `admission_jury_seed_grind`, the self-licensing residual of
`sybil_bond_private_fork_frontier`, and the seed half of `private_self_licensing_branch`.
*next:* `pol/lottery::SeedRing`, which can be built only from `SeedLeaf`s that `consensus/claims`
mints from `FinalClaim`s reaching `Final` after the seeded claim's accepting block (POL-R9,
CLAIM-R12). **Type:** `SeedLeaf`. The source is the owner's decision OQ-1 (00 §10), together with a
bootstrap rule (INV-PANEL-12); the predictability residual is OQ-26.
*t12:* **violated** (`core/palw_panel_v2.rs:1480-1510`, `:1664-1684`; `core/hashing/header.rs:165-174`;
`core/palw_attempt_v2.rs:213`, `:233-235`, `:528-538`; all *re-checked*;
`core/palw_fp_beacon_v3.rs:119-126`).
*Test:* `inv_pol_03_seeds_are_not_rerollable`, property plus simulation: vary a claim's id (fake
roots), any header field and the timing of other producers' claims; assert the panel changes only
with the ring and the admission index.

**INV-POL-04 — a claim's work equals the chain's derivation from the effective target and the
per-draw work.**
No producer or registrant input may enter it.
*Stops:* `declared_canonical_job_weight_inflation`, `declared_class_target_free_weight`,
`fp_self_reported_work`, `attention_geometry_price_inflation`.
*next:* `pol/lottery::derive_work`. **Type:** `DerivedWork` has a private constructor (POL-R7,
POL-R15).
*t12:* **holds** past `palw_canonical_work` (`core/palw_admission_v2.rs:452-473`).
*Test:* `inv_pol_04_pwu_is_the_derivation`, property.

**INV-POL-05 — one execution commitment backs at most one non-retired claim on a chain.**
*Stops:* `one_execution_many_claims`, `borrowed_root_claim` (in part). *next:* a rooted work-identity
index (POL-R8). *t12:* **holds** (`core/palw_state_v2.rs:28336-28360`).
*Test:* `inv_pol_05_one_execution_one_claim`, property plus simulation.

**INV-POL-06 — every counted `Valid` attests the whole job that the claim's recorded
`job_identity` names**, meaning its prompt, prefill and decode shape, not only its id.
*(Sharpened: the basis-≥-2 half belongs to INV-PANEL-01.)*
*Stops:* `short_job_same_job_id`, `honest_seats_slashed_by_unaligned_checks`.
*next:* `pol/verification::check_receipt` compares the receipt against `job_identity` (POL-R12).
*t12:* **holds** (ADR-0117 D3; basis recount `core/palw_state_v2.rs:2920-2924`; the seat-side check
was not re-read [unverified]).
*Test:* `inv_pol_06_a_valid_covers_the_named_job`, property.

**INV-POL-07 — a claim whose roots are not the named job's execution is convictable, not merely
voidable, within the class's verification deadline.**
*Stops:* `private_fake_root_burst`, `borrowed_root_claim`, `held_attention_lie_unattributable`.
*next:* `job_identity` is recorded at admission (POL-R13), and `pol/verification::adjudicate` works
from the claim record and served material alone.
*t12:* **partial**. `job_identity` is recorded (`core/palw_state_v2.rs:28478-28486`); automatic filing
is *pending* (`rcore/p2-file`); a held-attention lie is not yet provable.
*Test:* `inv_pol_07_fake_roots_are_attributable`, simulation.

**INV-POL-08 — the ticket target in force at a block is a function of its `SafeDaa` only.**
*Stops:* `private_work_target_easing`. *next:* `pol/lottery::target_in_force(at: SafeDaa)` (POL-R6).
*t12:* **violated (bounded by `W₀`)** (`core/palw_state_v2.rs:22736-22775`,
`core/palw_work_target_v1.rs:87-110`).
*Test:* `inv_pol_08_target_reads_the_safe_context`, property.

### 3.5 PANEL — draws, receipts, licences

**INV-PANEL-01 — no `VerifiedClaim` exists with `basis_k < 2` or with fewer than `q` counted `Valid`
seat indices.**
Coverage, quorum and `basis_k` count distinct seat indices. Each receipt names its seat index
(05 PANEL-R12), so one account holding two seats contributes two, and a seat index counts once.
*Stops:* `optimistic_single_seat_licence`, `sampled_verification_evasion`.
*next:* `pol/verification::check_licence` returns a `LicenceProof` with a private constructor, and
`consensus/claims::verify` needs that token (PANEL-R14).
*t12:* **holds** (`core/palw_state_v2.rs:2920-2933`, `:23756-23771`).
*Test:* `inv_panel_01_verified_requires_basis_two_and_quorum`, property.

**INV-PANEL-02 — a claim's panel is unchanged by any header field a producer can vary without a
new winning execution, and by the claim's own id.**
This holds under either seed source of OQ-1; under 04 POL-R9 the panel depends only on the ring, the
admission index, the draw index and the population snapshot.
*Stops:* `panel_draw_seed_grind`. *next:* `pol/panel::panel_seed` takes no header (PANEL-R7).
*t12:* **violated** (see INV-POL-03, *re-checked*).
*Test:* `inv_panel_02_anchor_nonce_and_timestamp_do_not_move_the_panel`, property that varies the
nonce within its bucket and the timestamp.

**INV-PANEL-03 — the executor's account, operator and key never hold a seat on its own claim.**
*(Aligned with 02 BOND-R3: t12's "no operator holds two seats" is dropped, because an account may
hold several seats under draws with replacement. See OQ-6.)*
*Stops:* `executor_judges_own_claim`. *next:* `consensus/bonds::draw_seats` excludes the executor
(PANEL-R9).
*t12:* **holds** (`core/palw_panel_v2.rs:1052`).
*Test:* `inv_panel_03_executor_and_its_operator_never_sit`, property.

**INV-PANEL-04 — a `Valid` quorum and an `Unavailable` quorum can never both form on one panel.**
*Stops:* `dual_quorum_opposite_licences`. *next:* `validate_params` refuses `2q ≤ n` (PANEL-R6).
*t12:* **holds** (`core/palw_panel_v2.rs:355`).
*Test:* `inv_panel_04_valid_and_unavailable_quorums_are_disjoint`, unit.

**INV-PANEL-05 — every account in a draw's population was registered, and mature, strictly before
the seeded claim's accepting block, hence before any leaf of its ring existed.**
The draw lays accounts out in registration order, so a key ground against a predicted seed moves
nothing.
*Stops:* `post_anchor_grinding`, `sybil_bond_private_fork_frontier`. *next:* PANEL-R8, BOND-R3,
BOND-R4.
*t12:* **holds** (`core/palw_panel_v2.rs:272-292`, `:1047`; `core/palw_state_v2.rs:16928-16943`).
*Test:* `inv_panel_05_population_predates_the_seed`, property.

**INV-PANEL-06 — an outsider-judged claim never becomes a `VerifiedClaim` without its outsider's
`Valid`.**
*Stops:* `registrant_self_certifying_panel`. *next:* PANEL-R14, item 5.
*t12:* **holds** (`core/palw_state_v2.rs:660`, `:25282`, `:25704`, `:25775`;
`core/palw_panel_v2.rs:2573`).
*Test:* `inv_panel_06_outsider_valid_is_required`, property.

**INV-PANEL-07 — node licence selection returns a set iff the acceptance predicate accepts some
subset of the pool.**
*Stops:* `licence_assembler_stall`. *next:* `pol/panel::select_licence` is defined through
`verify_licence` (PANEL-R17).
*t12:* **holds** (`core/palw_panel_v2.rs:2594-2766`; `pipeline/virtual_processor/processor.rs:7455`,
`:7625`).
*Test:* `inv_panel_07_selection_is_the_acceptance_predicate`, property.

**INV-PANEL-08 — a licence of basis < 2 carries no weight, reward, seat pay or release.**
*Stops:* `optimistic_single_seat_licence`. *next:* PANEL-R15.
*t12:* **holds** (`core/palw_state_v2.rs:2958-2975`, `:23756-23771`); seat pay on a partial licence is
[unverified].
*Test:* `inv_panel_08_basis_one_licence_has_no_authority`, property.

**INV-PANEL-09 — a `Candidate` class admits no claim.**
*Stops:* `admission_jury_sybil_capture` (bounds its consequence), `uncertified_class_fake_weight`.
*next:* `pol/panel::lifecycle_step` (PANEL-R3).
*t12:* **holds** (`core/palw_model_registry_v1.rs:329-380`, `:483-510`).
*Test:* `inv_panel_09_candidate_class_admits_nothing`, unit.

**INV-PANEL-10 — node and fold derive one court shape, turn deadline and eligibility from the same
inputs.**
*Stops:* `panel_arity_mismatch_defaults_honest`. *next:* `pol/panel` exports `court_shape` once,
and a crate-graph test checks that node code calls it (PANEL-R22).
*t12:* **violated** (`kaspad/src/palw_panel.rs:6749` against
`pipeline/virtual_processor/processor.rs:11332`); the fix is *pending* (`d2752b54`).
*Test:* `inv_panel_10_node_and_fold_derive_one_court_shape`, unit plus vector.

**INV-PANEL-11 — no transition charges a seat for silence.**
*Stops:* `seat_silence_mispriced`. *next:* PANEL-R18.
*t12:* **holds** (`core/palw_state_v2.rs:19164-19171`). The corollary, that the producer is charged
instead, is `silent_quorum_griefing`.
*Test:* `inv_panel_11_silence_charges_no_seat`, property.

**INV-PANEL-12 — panels stay live.** *(added in synthesis)*
If honest producers are online and honest eligible stake of at least `1 − s_target` is online, some
claim is licensed within `L_live = W_ring + 2·D(c)` ticks of `LocalDaa` from any state, with honest
heartbeats claiming every slot (10 §2.2 H6). Here `W_ring` is the seed-ring wait and `D(c)` the
class's verification deadline (04 POL-R11), counted twice for one redraw. That includes genesis and
the first block after a licence halt. `validate_params` computes `L_live`, and the simulation
asserts it. *(Review: the draft said "a bounded span" with no bound.)*
*Why:* a seed rule that needs `FinalClaim`s produced after a pending claim (04 POL-R9) has no
bootstrap: panels wait for Finals, and Finals wait for panels (`seed_ring_bootstrap_deadlock`). t12
also froze its panels once by retiring bonds.
*Stops:* `seed_ring_bootstrap_deadlock`, `exit_freeze_by_retirement`,
`replay_budget_horizon_collapse`, `panel_room_zero_readiness_halt`, `anchor_bind_censorship`,
`private_readiness_lapse_panel_capture`.
*next:* the seed rule chosen under OQ-1 MUST include a bootstrap and halt rule that this test
exercises (PANEL-R6, PANEL-R10, POL-R9).
*t12:* **unknown**. Seeds are not the constraint on t12. `exit_freeze_by_retirement` was fixed (A §3.5).
t12 met the bootstrap problem in its seat-maturity floor and waived that floor while fewer than
`depth` anchors exist (`core/palw_panel_v2.rs:660-667`). `anchor_bind_censorship` is **closed** at
the reference: the chain derives the panel bindings a block owes, so no producer can withhold one
(`pipeline/virtual_processor/processor.rs:11814-11828`, `:12290`, `:2053`;
`core/palw_state_v2.rs:23733-23735`, *re-checked*).
*Test:* `inv_panel_12_panels_bind_from_genesis_and_after_a_halt`, simulation.

### 3.6 COURT — adjudication, defaults, rewards

**INV-COURT-01 — an unadjudicable proof yields neither a conviction nor an acquittal.**
*Stops:* `court_never_convicts`, `self_chosen_close_step_acquittal`. *next:* `adjudicate` returns
`Unadjudicable` (COURT-R1). *t12:* **holds** (`core/palw_court_v2.rs:6-17`).
*Test:* `inv_court_01_unadjudicable_proof_mints_no_verdict`, vector.

**INV-COURT-02 — a default convicts only the defaulting party and is never a basis against signers.**
*Stops:* `deliberate_court_loss_slashes_signers` (the default half). *next:* `ConvictionBasis::Default`
names only the defaulting party (COURT-R3).
*t12:* **holds** (`core/palw_state_v2.rs:3805-3818`). The dissection half, reason 8, is *pending*
(`3ee76a99`).
*Test:* `inv_court_02_default_is_never_a_signer_basis`, property.

**INV-COURT-03 — every conviction's evidence rebuilds the claim's committed execution root, or proves
a job-identity fault.**
*Stops:* `accuser_authored_binding_conviction`, `borrowed_root_claim`,
`forged_false_valid_equivocation`. *next:* one adjudicator with the attribution chain (COURT-R4).
*t12:* **holds** for offence kinds 3 and 4 (`core/palw_offence_attribution_v1.rs:1-50`); the court
paths were not re-traced.
*Test:* `inv_court_03_conviction_rebuilds_committed_root`, vector.

**INV-COURT-04 — an open session never prevents or delays an admissible accusation on the same claim
beyond its bound.**
*Stops:* `decoy_dissection_preemption`, `da_court_single_session_preemption`. *next:* COURT-R5.
*t12:* **violated** (`core/palw_state_v2.rs:23985-23987`); the fix is *pending* (`9bf7accf`).
*Test:* `inv_court_04_decoy_session_blocks_no_accusation`, simulation.

**INV-COURT-05 — a challenger defeated on data the accused committed is made whole when that data is
convicted.**
*Stops:* `forger_race_challenger_forfeit`, `held_attention_consistent_forger`. *next:* COURT-R6.
*t12:* **violated** (`core/palw_state_v2.rs:25516-25518`); the fix is *pending* (`c68479db`).
*Test:* `inv_court_05_acquittal_on_forged_anchor_is_restored`, simulation.

**INV-COURT-06 — the worst honest prosecution of every admitted class fits `window_court` at its
derived turns.**
*Stops:* `accepted_but_unprosecutable_claim`, `span_unit_mismatch_short_windows`.
*next:* `turn_deadline` and a `validate_params` check (COURT-R7).
*t12:* **partial** (`core/palw_fp_devnet_v3.rs:175-196`; `core/palw_court_deadline.rs:14-18`); the
per-class compute turn is *pending*.
*Test:* `inv_court_06_worst_honest_prosecution_fits_window`, property.

**INV-COURT-07 — a reporter reward is paid only against a commitment that preceded the reveal on the
branch.**
*Stops:* `reporter_reward_front_running`. *next:* COURT-R8, ECON-R10.
*t12:* **unknown**. The objects fold (`core/palw_state_v2.rs:25719-25734`), but
`apply_reporter_revealed` was not read.
*Test:* `inv_court_07_copied_reveal_earns_nothing`, simulation.

**INV-COURT-08 — each conviction conserves value.**
Per conviction, the debit collected equals the amount burned plus the reporter reward paid. No
conviction mints. INV-BOND-09 is the ledger form of the same property.
*Stops:* `self_report_capture` (through ECON-R10), and slash-path inflation.
*next:* `consensus/bonds::apply_slash` returns `Collected` (BOND-R9, BOND-R11).
*t12:* **unknown** (not traced).
*Test:* `inv_court_08_slashing_conserves_value`, property.

### 3.7 BOND — stake, commitments, exit, vesting

**INV-BOND-02 — the distribution of the number of seats an owner controls depends only on the owner's
share of eligible stake, not on how anyone partitions stake into accounts.**
*Stops:* `bond_split_amplification`, `undetected_coverage_lie`, `seat_sybil_by_cheap_identities`.
*next:* `consensus/bonds::draw_seats` makes independent draws with replacement, with no cap
(BOND-R3).
*t12:* **violated**. The draw is successive sampling without replacement, one seat per operator, with
a 1M cap (`core/palw_panel_v2.rs:35`, `:137`, `:1729`, *re-checked*; `:1828-1860`). At 17.29M MSK,
one operator gets P2 = 0 while 133 operators get 0.5003 (ADR-0152 §4.3).
*Test:* `inv_bond_02_seat_distribution_is_split_invariant`, property (a chi-square test against
`Bin(n, s)`).

**INV-BOND-03 — for every account-level allowance `f`, `Σ f(parts) ≤ f(Σ parts)`.**
The allowance must be superadditive, rounded down, with no constant term, no ceiling division and no
cap. This invariant absorbs the linearity half of the retired INV-CLAIM-06.
*Stops:* `bond_split_amplification`, `strike_evasion_by_split`. *next:* BOND-R2, BOND-R15, CLAIM-R6.
*t12:* **violated** (`core/palw_state_v2.rs:12239`, `:967`; `core/palw_panel_v2.rs:137`; all
*re-checked*).
*Test:* `inv_bond_03_no_allowance_is_superlinear_in_accounts`, property.

**INV-BOND-04 — no stake leaves an account while anything it signed or produced is inside its
conviction horizon.**
*Stops:* `lock_escape_before_conviction`, `second_clock_heartbeat_escape`.
*next:* `consensus/bonds::exit_permitted(.., chain_final: ChainFinalizedDaa)` (BOND-R14). **Type:**
`ChainFinalizedDaa`, which the node's `FinalizedDaa` cannot be passed as.
*t12:* **partial**. The v6 gate holds (`core/palw_state_v2.rs:2796-2821`), but it reads `LocalDaa`
and escapes to DAA only (`:2224-2230`).
*Test:* `inv_bond_04_exit_waits_for_every_horizon`, simulation.

**INV-BOND-05 — a vesting row is burnable from `Final` until its release.**
Its release reads `ChainFinalizedDaa` and the licence count, never `LocalDaa`, `SafeDaa` or the node's
`FinalizedDaa`. It is never collateral.
*Stops:* `vesting_escape`, `second_clock_heartbeat_escape`. *next:* `vesting_releasable` with
`expiry: Deadline<ChainFinal>` (BOND-R12).
*t12:* **partial** (`core/palw_state_v2.rs:19200-19324`, `:19948-19972`; `core/palw_vesting_v1.rs:445-447`,
`:454-464`).
*Test:* `inv_bond_05_private_daa_does_not_release_vesting`, simulation.

**INV-BOND-06 — `committed + accuser ≤ posted` and `committed ≤ ⌊ratio·posted⌋` after every block.**
This invariant absorbs the ceiling half of the retired INV-CLAIM-06.
*Stops:* `lock_ledger_double_backing`, `one_bond_backs_unbounded_immature_work`.
*next:* one room function (BOND-R6, BOND-R18).
*t12:* **holds** (`core/palw_state_v2.rs:2601-2616`, `:28502-28530`; admission at
`core/palw_admission_v2.rs:676-700`).
*Test:* `inv_bond_06_one_invariant_at_every_gate`, property.

**INV-BOND-07 — `seats · duty ≤ commitment_at_bind` for every class.**
The withholding amplification is therefore at most 1.
*Stops:* `withholding_producer_uncharged`. *next:* `seat_duty` (BOND-R7).
*t12:* **holds** (`core/palw_state_v2.rs:2904-2906`).
*Test:* `inv_bond_07_duty_never_exceeds_forfeit`, property.

**INV-BOND-08 — the amount a conviction takes for one claim-bound offence does not decrease when the
same stake is split across more accounts.**
*Stops:* `action_tier_dilution`. *next:* `action_tier` depends on the claim only (BOND-R10).
*t12:* **violated** (`core/palw_state_v2.rs:949-953`, `:3294-3316`).
*Test:* `inv_bond_08_tier_is_split_invariant`, property.

**INV-BOND-09 — `Σ posted + Σ unbonding + Σ slashed_total` changes only by deposits and completed
exits.**
Slashed value is burned and counted. INV-COURT-08 is the per-conviction form.
*Stops:* slash redistribution, a design hazard with no catalog entry. *next:* `apply_slash`
(BOND-R9, BOND-R11).
*t12:* **holds** (`core/palw_state_v2.rs:18719-18729`, `:3416-3420`).
*Test:* `inv_bond_09_slashes_are_burned_and_counted`, property.

### 3.8 ECON — supply and the design bar

**INV-ECON-02 — genesis mints exactly 10B MSK, and `genesis + Σ minted ≤ 25B MSK` on every branch.**
*Stops:* `execution_quanta_overmint`, `algo4_credit_mint_holes`, `model_market_payout_unwithheld`.
*next:* `consensus/validation` pays only admitted attempts at the per-claim rate tied to the
schedule (ECON-R16) and checks every coinbase against the rooted minted counter (ECON-R17).
*t12:* **partial**. The genesis cap and the schedule table are bounded (`core/config/premine.rs:48-53`,
`:752-756`, `:781-793`; `processes/coinbase.rs:679-703`; `core/constants.rs:37-45`). But the table's
test assumes one paid block per target block time (`processes/coinbase.rs:702`, *re-checked*),
while paid attempt blocks per DAA are only the `W` controller's target
(`core/palw_state_v2.rs:22756`, *re-checked*) plus the floor class. Unentitled merged blues are not
paid (`pipeline/virtual_processor/processor.rs:5436-5591`), with a stated one-mergeset residual race
(`:5540-5546`). *(Review: the draft said "holds" on the table's test alone.)*
*Test:* `inv_econ_02_supply_never_exceeds_the_cap`, property plus vector.

**INV-ECON-03 — each block's subsidy is split exactly, and each withheld escrow resolves exactly
once.**
*Stops:* `merged_work_payout_mismatch`. *next:* `split_subsidy` and the supply ledger (ECON-R4).
*t12:* **partial**. The split is exact (`core/dns_finality.rs:3054-3063`) and the coinbase withholds
the escrow (`processes/coinbase.rs:150-167`), but the resolution identity exists only in a test
(ADR-0152 V-3).
*Test:* `inv_econ_03_every_withheld_sompi_resolves_once`, property.

**INV-ECON-04 — every destroyed sompi is recorded in the supply ledger by path.**
*Stops:* `unbound_model_sink_output_burn`. *next:* one rooted supply ledger (ECON-R6).
*t12:* **violated** (`processes/transaction_validator/tx_validation_in_isolation.rs:102-111`;
`processes/transaction_validator/tx_validation_in_header_context.rs:95-110`;
`core/palw_state_v2.rs:19227-19228`; `processes/coinbase.rs:138`, `:148`).
*Test:* `inv_econ_04_no_silent_burn`, property.

**INV-ECON-05 — a block without work mints nothing, and raising a branch's DAA never raises a block's
subsidy.**
*Stops:* `heartbeat_clock_acceleration` (the issuance half). *next:* `block_subsidy(daa: BlockDaa,
lane, ..)` (ECON-R2).
*t12:* **holds** (`pipeline/body_processor/body_validation_in_context.rs:74-89`;
`processes/coinbase.rs:579-620`).
*Test:* `inv_econ_05_heartbeats_mint_nothing`, property.

**INV-ECON-06 — no leg of a claim's value leaves consensus control before its conviction horizon.**
*Stops:* `early_extraction`, `quantum_maturity_reads_wall_clock`. *next:* ECON-R7.
*t12:* **violated** (`core/palw_state_v2.rs:19243`; `core/palw_economic_safety_v1.rs:95-113`,
`:165-179`).
*Test:* `inv_econ_06_no_leg_leaves_before_the_horizon`, simulation.

**INV-ECON-07 — for each admitted class and door, an undetectable fraud succeeds with probability
below `P*` whenever the attacker's share of eligible stake is below `s_target`.**
*Stops:* `undetected_coverage_lie`. *next:* `stake_threshold`, and `admit_class_econ` part (d)
(ECON-R11).
*t12:* **partial**. The floor class is EV-positive only above 17.29M MSK of Sybil stake (12.74M in
the worst state); the 8k class above about 6.63M, a derived figure (ADR-0152 §4.3).
*Test:* `inv_econ_07_undetectable_fraud_is_ev_negative_below_target_share`, property.

**INV-ECON-08 — a reporter reward is strictly less than the debit it is carved from.**
*Stops:* `self_report_capture`. *next:* `reporter_reward` (ECON-R10).
*t12:* **holds** (`core/palw_state_v2.rs:970`).
*Test:* `inv_econ_08_self_report_loses`, property.

**INV-ECON-09 — the reservation, the ceiling, the forfeit and the weight price of a claim read one
number in one unit.**
*Stops:* `collateral_unit_mismatch`, `weight_unit_gap`. *next:* ECON-R12, ECON-R14.
*t12:* **partial** (`core/config/premine.rs:107-116`, `:120-128`).
*Test:* `inv_econ_09_one_unit_for_reservation_and_weight`, property.

**INV-ECON-10 — a class that fails `admit_class_econ` takes no claim.**
*Stops:* `unattributable_2m_claims`, `held_attention_lie_unattributable`. *next:* ECON-R11, PANEL-R5.
*t12:* **partial** (`core/palw_work_target_v1.rs:217-228`); the 2M closure is *pending*.
*Test:* `inv_econ_10_unadmitted_class_takes_no_claim`, unit.

### 3.9 FORK — the comparator

**INV-FORK-02 — `compare_chains` is a strict total order on distinct admissible tips.**
*Stops:* `pairwise_context_cycle`. *next:* `ForkKey` ends in the tip id (FORK-R5).
*t12:* **holds** for the comparator (`core/palw_fork_choice.rs:72-78`), which is not the selector.
*Test:* `inv_fork_02_comparator_is_a_strict_total_order`, property (symmetry, totality,
transitivity).

**INV-FORK-03 — selection is a function of (finalized anchor, candidate set) only.**
There is no incumbent, no arrival order and no node-local state.
*Stops:* `path_dependent_sink_split`, `prior_sink_weight_divergence`, `dns_gate_node_local_abstain`.
*next:* `select_tip(ctx, candidates, params)`. **Type:** no `&mut` and no store handle (FORK-R2,
FORK-R11, FORK-R13).
*t12:* **violated** (`pipeline/virtual_processor/processor.rs:13636-13644`, *re-checked*; `:13219`,
`:13234-13249`; `pipeline/virtual_processor/dns_bft.rs:457`, `:532-536`).
*Test:* `inv_fork_03_selection_is_path_independent`, property (determinism, reorg invariance,
permutation invariance).

**INV-FORK-04 — the relative order of two candidates does not depend on any other candidate.**
*Stops:* `junk_candidate_context_drag`, `stalled_leader_context_drag`. *next:* FORK-R3.
*t12:* **partial**. The comparator is pairwise (`core/palw_fork_choice.rs:72-78`), but the sink
depends on heap order and on the previous sink.
*Test:* `inv_fork_04_no_candidate_can_move_the_order_of_two_others`, property.

**INV-FORK-05 — no block without verified work increases `Ω` or `V`.**
That covers heartbeats, execution-lane blocks, failed or skipped attempts and unverified claims. Such
a block increases `safe` only if it carries a proven verdict that releases a dispute hold. The
passage of time it adds can only void pending claims.
*Stops:* `failed_lottery_blue_weight`, `heartbeat_padding_buys_frontier_key`,
`unverified_live_weight`, `heartbeat_clock_acceleration`.
*next:* `Weight` is minted only by the claim state machine (FORK-R7, FORK-R9).
*t12:* **violated** (`processes/ghostdag/protocol.rs:666-671`; `core/palw_state_v2.rs:28436-28443`,
`:18650-18654`).
*Test:* `inv_fork_05_blocks_without_verified_work_add_no_weight`, property plus simulation.

**INV-FORK-06 — every chain-selection site returns the same tip for the same inputs.**
The sites are the tip, the template parent, IBD and staging, headers-proof acceptance, restart and
bootstrap recovery, and the DAG selected parent.
*Stops:* `selection_sites_disagree`, `ibd_asymmetric_weighing`, `failed_lottery_blue_weight`.
*next:* FORK-R10, FORK-R14.
*t12:* **violated** (`processes/ghostdag/protocol.rs:216-220`;
`processes/pruning_proof/validate.rs:488-551`; `protocol/flows/src/flowcontext/bootstrap_recovery.rs:274-300`;
`core/palw_fork_authority_v2.rs:43-45`; all *re-checked*).
*Test:* `inv_fork_06_every_selection_site_agrees`, property plus vector.

**INV-FORK-07 — a candidate that does not contain the finalized anchor is never selected.**
*Stops:* `long_range_rewrite`, `dns_veto_expires_on_heartbeat_clock`. *next:* `admit` is the only
constructor of `Admissible` (FORK-R1).
*t12:* **partial** (`pipeline/virtual_processor/processor.rs:13704`, `:13772`;
`pipeline/virtual_processor/dns_bft.rs:554-563`).
*Test:* `inv_fork_07_no_candidate_below_the_finalized_anchor_is_selected`, property.

**INV-FORK-08 — burying a claim or releasing a hold never lowers a chain's key.**
*Stops:* `fresh_tip_unresolved_fallback`. *next:* `live = safe + ⌊β(V − safe)⌋`.
*t12:* **holds** (`core/palw_fork_choice.rs:56-64`).
*Test:* `inv_fork_08_maturing_never_lowers_the_key`, property.

**INV-FORK-09 — advancing the finalized anchor leaves the pairwise order of every candidate that
contains the new anchor unchanged.**
*next:* the keys are absolute totals. *t12:* **n/a** (no anchor context).
*Test:* `inv_fork_09_anchor_advance_preserves_the_order`, property.

### 3.10 FINAL — finality and pruning

**INV-FINAL-01 — a node's finalized anchor never reverts.**
Every later anchor descends from, or equals, every earlier one. The admissibility half, that no
selected tip lacks the anchor, is INV-FORK-07.
*Stops:* `long_range_rewrite`, `dns_veto_expires_on_heartbeat_clock`. *next:*
`consensus/finality::advance_finality` (FINAL-R4, FINAL-R5).
*t12:* **partial**. The header pruning point is monotone (`processes/pruning.rs:106-156`), but the
only vote-based anchor expires (`pipeline/virtual_processor/dns_bft.rs:554-563`;
`core/dns_finality.rs:1464`).
*Test:* `inv_final_01_the_finalized_anchor_never_reverts`, simulation.

**INV-FINAL-02 — the node's anchor moves only by `advance_finality` on a selected chain.**
No timer, overlay or node-local flag moves it.
*Stops:* `dns_veto_expires_on_heartbeat_clock`, `dns_gate_node_local_abstain`. *next:* FINAL-R4,
FINAL-R8, FINAL-R13.
*t12:* **violated** (`pipeline/virtual_processor/dns_bft.rs:457`, `:534-536`, `:554-563`).
*Test:* `inv_final_02_only_selection_advances_finality`, property.

**INV-FINAL-03 — `block_finalized_anchor` is monotone along a chain and a pure function of the chain.**
*Stops:* `pruning_point_disagreement`. *next:* FINAL-R3, FINAL-R6.
*t12:* **partial** (`processes/pruning.rs:106-156`;
`pipeline/virtual_processor/processor.rs:3958-3978`).
*Test:* `inv_final_03_block_finalized_anchor_is_monotone_and_pure`, property plus vector.

**INV-FINAL-04 — every claim accepted at or below the finalized anchor's own claim frontier is
resolved in the anchor's own state.**
Resolved means terminal, or `FinalClaim` with `conviction_ends` elapsed. The claim frontier covers
every claim, including unverified ones, disputed ones and `Final` ones still convictable (07 §2.3). It
is therefore resolved identically on every admissible candidate. *(Review: under the draft's frontier,
over verified entries only, this was false by construction.)*
*Stops:* `pruning_deletes_evidence`. *next:* `FinalizedAnchor::own_frontier` (FINAL-R10).
*t12:* **partial** (`pipeline/pruning_processor/processor.rs:223-228`;
`core/palw_fork_authority_v2.rs:66-77`).
*Test:* `inv_final_04_nothing_unresolved_below_the_anchors_own_frontier`, property.

**INV-FINAL-05 — neither blocks without verified work nor the passage of `LocalDaa`, `SafeDaa`, blue
score or time makes a block finalizable.**
Finality counts safe anchors, and safety is clock-free (06 §3.3). A `Final` status, timed on
`SafeDaa`, does not enter.
*Stops:* `blue_depth_unit_mismatch`, `heartbeat_clock_acceleration`,
`private_daa_finality_acceleration`. *next:* `is_finalizable` counts settled anchors and `Ω` only
(FINAL-R2, FINAL-R7); `VerifiedEntry` carries no `Final` flag.
*t12:* **violated** (`core/config/params.rs:2781`, `:2697-2717`; `processes/block_depth.rs:55-80`;
`processes/pruning.rs:143`).
*Test:* `inv_final_05_empty_blocks_do_not_finalize`, property.

**INV-FINAL-06 — the pruning point is at or below the finalized anchor's own claim frontier.**
It is the same pure function that headers commit, and it never deletes evidence that an unresolved
claim on any admissible candidate can need.
*Stops:* `pruning_point_disagreement`, `pruning_deletes_evidence`. *next:* `pruning_point`
(FINAL-R10).
*t12:* **partial** (`processes/pruning.rs:106-156` against
`pipeline/pruning_processor/processor.rs:223-228`).
*Test:* `inv_final_06_pruning_never_passes_the_anchor`, property.

**INV-FINAL-07 — a conflicting finalization costs more than it can release.**
For a node that has not yet finalized an anchor to finalize a conflicting one, an attacker needs a
branch that wins chapter 06 and carries `k_final` safe anchors and `d_final` of verified weight. On a
branch the attacker produces, those cost bucket tokens and self-licences (`P_cap`, 10 §2.1 C11), not
inference. The parameters MUST make that cost, and the time the bucket needs to admit it, exceed the
value the split could release. This ties back to INV-ECON-01.
*Stops:* `long_range_rewrite`, `private_branch_double_spend`, `private_self_licensing_branch`.
*next:* `validate_params` derives `k_final` and `d_final` (07 Q1).
*t12:* **unknown**. t12 has no finality depth denominated in PoL.
*Test:* `inv_final_07_a_conflicting_finalization_costs_more_than_it_pays`, simulation.

### 3.11 BLK — block structure and the per-block transition *(added in review)*

**INV-BLK-01 — the per-block transition is one ordered pure function.**
A chain block's state MUST equal `apply_block(parent_state, block, mergeset, ClockContext)`, computed
in the order of 11 §4. Every rule in it reads the one `ClockContext(B)`, whose `SafeDaa` and
`ChainFinalizedDaa` are functions of the parent's state (01 DAA-R18). Archival, pruned and snapshot
nodes MUST compute equal state roots.
*Why:* a rule that read a clock its own block's objects move would depend on the order of its own
steps, and two nodes that ordered them differently would split.
*Stops:* `node_local_input_in_fold`, `clock_reference_node_local`, `merged_work_payout_mismatch`.
*next:* `consensus/validation::apply_block` (11 §4). **Type:** `ClockContext` is built once, before
step 1, and passed by shared reference.
*t12:* **partial**. The fold is pure and ordered (`core/palw_state_v2.rs:19719-19722`, steps at
`:20531-20949`). But it reads the block's own DAA score, and the panel bindings it folds are derived
by the processor before it (`pipeline/virtual_processor/processor.rs:12290`).
*Test:* `inv_blk_01_block_transition_is_one_pure_ordered_function`, vector (archival, pruned and
snapshot replays) plus property.

**INV-BLK-02 — an attempt that is not admitted confers nothing on the block that carries it.**
A block whose own attempt is refused is on no selected chain, so it claims no slot and is paid
nothing. A merged attempt that fails admission is skipped and paid nothing. A lost-lottery header is
invalid.
*Why:* a refused attempt costs a fabricator a few hashes (04 §2.4).
*Stops:* `failed_lottery_blue_weight`, `merged_work_payout_mismatch`, `bondless_attempt_row_grind`.
*next:* 11 BLK-R2, BLK-R5, BLK-R6; 08 ECON-R16.
*t12:* **partial**. A refused own attempt disqualifies the block from the chain
(`core/palw_attempt_v2.rs:1129-1131`). A merged one is skipped (`pipeline/virtual_processor/processor.rs:11630-11652`)
and not paid (`:5436-5591`). Both keep their `2^20` of blue work (INV-CLAIM-04).
*Test:* `inv_blk_02_refused_attempts_tick_nothing_and_are_paid_nothing`, property.

## 4. Reconciliation record

**Retired.** INV-CLAIM-06 ("per-bond exposure ≤ ρ·collateral and every per-bond claim limit is
collateral-linear") merged into INV-BOND-06 (the ceiling) and INV-BOND-03 (linearity). Its test
`inv_claim_06_bond_exposure_is_collateral_linear` is not written. 03 CLAIM-R6 now cites the two
bond invariants. The number stays retired and will not be reused.

**Test-name choices** (one name per ID, taken from the owning chapter):

| ID | canonical test | name no longer used | where it appeared |
| --- | --- | --- | --- |
| INV-FORK-01 | `inv_fork_01_private_daa_does_not_buy_comparative_maturity` | `inv_fork_01_private_local_daa_buys_no_maturity` | 01 (edited) |
| INV-BOND-01 | `inv_bond_01_split_does_not_raise_issuance_or_capacity` | `inv_bond_01_split_is_neutral` | 03 (edited) |
| INV-ECON-01 | `inv_econ_01_extracted_never_exceeds_frozen` | `inv_econ_01_colluding_quorum_locks_exceed_gain` | 05 extraction; becomes one case |

**Statements changed in synthesis:**

| ID | change | reason |
| --- | --- | --- |
| INV-CLAIM-04 | "fork-choice weight above ε" became "block-level work above ε, and no header-only quantity in any fork key" | under 06 FORK-R9 header work is never fork-choice weight |
| INV-CLAIM-07 | the bound is restated with `β_u` and `β_v`, allowing `β_v ≤ 1` | 06 FORK-R16 allows `β = 1`, and FORK-R7 sets unverified weight to zero |
| INV-CLAIM-08 | now "`FinalClaim` only from `VerifiedClaim`"; the basis condition moves to INV-PANEL-01 | avoids stating one property twice |
| INV-POL-06 | now only "a Valid covers the named job"; the basis condition moves to INV-PANEL-01 | the same |
| INV-PANEL-03 | "no operator holds two seats" dropped | 02 BOND-R3 draws with replacement (OQ-6) |
| INV-FINAL-01 | the admissibility clause moves to INV-FORK-07 | the same |
| INV-BOND-01 | t12 status changed from "holds" (02) to **partial** | 02's own table 6.2 shows the per-bond class share is split-sensitive |
| INV-POL-01 | t12 status set to **violated** (05 had "partial") | `Provisional` and counted licences carry equal live weight |
| INV-TIME-08, INV-PANEL-12 | added | the deadline-mark conflict between 01 and 03, and POL-R9's missing bootstrap |

**Changed in review** (00 §9, R16–R27):

| ID | change | reason |
| --- | --- | --- |
| INV-CLAIM-01, INV-TIME-06 | equalities became bounds (01 S5) | `SafeDaa` copies `LocalDaa`'s spacing; S4 was false for insertions |
| INV-FORK-01 | exact inequality `(safe, live)(A') ≤ (safe, live)(A)`; `Final` no longer enters `safe` | burial did not imply settlement under a `SafeDaa` challenge window |
| INV-TIME-04, INV-TIME-05, INV-TIME-07, INV-BOND-04, INV-BOND-05, INV-CLAIM-05 | `FinalizedDaa` split into the chain's `ChainFinalizedDaa` (state rules) and the node's `FinalizedDaa` (no state rule) | a state rule reading a node's anchor splits honest nodes |
| INV-CLAIM-04 | `ε` removed; a lost ticket is invalid; a refused attempt confers nothing | `ε` had no consumer |
| INV-CLAIM-07 | the bound counts non-voided verified weight not in `safe` | false under 06's formula |
| INV-CLAIM-10, INV-FINAL-04 | "resolved" defined; no cross-unit comparison with pruning | unit mixing; the frontier covered verified entries only |
| INV-POL-03 | price is one admitted, licensed claim per try; admission index is the per-claim input | a private branch self-licenses fake roots |
| INV-PANEL-01, INV-PANEL-05, INV-PANEL-12 | seat indices; population before the accepting block; a numeric liveness bound | review |
| INV-ECON-02 | t12 status holds → **partial** | paid blocks per DAA are controller-bounded |
| INV-BLK-01, INV-BLK-02 | added | no chapter specified the block transition (chapter 11) |

**Cross-chapter fixes that change what an invariant is checked against** (see 00 §9 for the full
list): 05's deadlines are now `Deadline<Safe>`, except the uncharged seed-ring wait (`Deadline<Local>`) and
the DA disclose deadline (`Deadline<ChainFinal>`, PANEL-R21). 06's `SafeDaa` is 01's licence-ring
definition. 01's per-block clock struct is `ClockContext`, and `SafeContext` means 06's anchor-only
context.
