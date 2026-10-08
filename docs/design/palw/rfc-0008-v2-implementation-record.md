# RFC-0008 v2 implementation record — claim-backed work slices on the weightless EXEC lane (DORMANT)

Branch `rfc8/x8-exec-v2` (worktree `MISAKA-wt-b/wc-x8`), base `c931df046`, written 2026-10-08 by lane X8; reviewed, integrated and
completed on `rfc8/x8r-review` by lane X8R (sections 10–12), merged with the integration line at `35a9ae1c8` (the UNSCHED reset: the
shipped t12 ids are int-12's). Normative text: [RFC-0008](../../rfc/0008-palw-claim-backed-consensus-blocks.md) and the
[implementation spec v1](rfc-0008-implementation-spec.md) with its **amendment 1** (section 10 there: the verification route and the
five design decisions). This record says what exists, what proves it, and what does not.

**The fence `palw_exec_payload_v2` is `None` on every ruleset and cannot be armed on a public one: `PALW_EXEC_PAYLOAD_V2_ARMABLE =
false`, so `validate_palw_v2` refuses every real height by name.** Only a salted drill of testnet-12 lifts that refusal
(`palw_exec_payload_v2_armable_on`), and every other refusal — the prerequisites, among them the kernel route whose own validation
refuses every height — still applies there. No shipped params id, fingerprint, schedule id or golden changed (the field is Some-only in
every hasher and `Some(never())` collapses out). Every pipeline test arms the fence by editing the `Config` it owns *after*
`ConfigBuilder::build`, the documented validation bypass; no node can. Nothing here is a statement that RFC-0008 is complete: the
capacity/liveness and recovery drills of spec section 9 are external, and section 12 names the code that remains.

Statuses (the matrix vocabulary): **IMPLEMENTED_AND_TESTED** (real node path, real pipeline test, behind the unarmed fence) ·
**DORMANT_NOT_INTEGRATED** (code with no node caller) · **CODE_GAP** · **DESIGN_GAP** · **EXTERNAL_GATE_PENDING**.

## 1. Commits

| commit | content |
| --- | --- |
| `6176a636a` | milestone 1: `PXE2` envelope and both subtype payloads, ledgers and pure rules, the fold (declaration, six-rule admission, settlement, void, expiry), the dormant fence, delta/object/tail allocation, core tests |
| `ebf7b0536` | the weightless carriage: anchor trailer `PXA2`, closure, header/body gates, virtual processing, template, verdicts, sync/IBD/orphan hooks, first pipeline tests |
| `48c90a4b3` | permit verdicts in canonical order (duplicate permit, payee bound); EXEC_TX, restart, replay, IBD, burst and orphan-pool tests |
| `e54330e33` | `WORK_SLICE` challenge subject checked against the shared contract crate (`misaka-palw-sdk` test) |
| `504aed522` | reorg test (stranded anchor leaves the window; republished slice credited once; a third node agrees) |
| `0dca2ca35` | RPC claim provenance names the new void reason (an exhaustive match in `kaspa-rpc-service` that the first commits broke) |
| `960f65a42` | kaspad: the operator's `EXEC_TX` producer signs `PXE2` from the fence; the two exhaustive matches in `kaspad/src/palw_panel.rs` (void reason, object name); this record; a pointer in the spec |
| `ad1f609ed` (X8R) | merge of the integration line `9ea89994b`: union-only conflicts; `WorkRootExpired` takes the explicit void number 130 |
| `253e5a355` (X8R) | the review (section 10): every un-gated pipeline path gated on the fence at the block's DAA; tag 130 rides exactly as an undecodable payload; P11, the orphan fence-off test |
| `966606811` (X8R) | amendment 1: the verification route through the G14 kernel route, suffix void (void reasons 131/132), leg cap, refusal record (delta note 181), RPC op 240, relay admission, the `EXEC_SLICE` producer, the drill-path armability, the kernel-route prerequisite |
| `fd9a7f22e` (X8R) | merge of the integration line `35a9ae1c8` (UNSCHED: ids back to int-12's) |

## 2. Allocation and wire facts

| item | value |
| --- | --- |
| object tag | 130 `ExecWorkRootOpenedV2` (tags 131-139 unused) |
| delta number | 180 `ExecV2Row { table, key, old, new }`, tables 1 roots, 2 slices, 3 jobs, 4 anchored; **181 `ExecV2Verdict { carrier, code }`**, the refusal record (a note: inert on apply and revert); 182-189 unused |
| state carriage tail | `0xEE` (`PALW_CARRIAGE_EXEC_V2_TAIL_V1`); the empty carriage is byte-identical to before; root block `exec_v2/v1` is Some-only |
| void reasons | **explicit borsh 130** `WorkRootExpired` (was the implicit 11; moved by X8R at the integration merge, beside RFC-0010's explicit 120–122), **131** `WorkSliceProvenFalse`, **132** `WorkSliceDefaulted` (amendment 1); pinned in `rcore_v22_skeleton::the_v22_void_reasons_are_pinned` and `exec_v2_fold_v1::the_object_and_delta_numbers_are_the_allocated_ones` |
| slice refusal codes | `PalwSliceRefusalV2::code`, 1–27 (0 = admitted), never renumbered (`refusal_codes_are_distinct_and_named`) |
| RPC | op **240** `getPalwExecV2Status` (lane range 240–249), gRPC messages 1294/1295; a versioned camelCase JSON observation (`PalwExecV2ObservationV1`, version 1) |
| verification binding domains | `misaka-palw/exec-v2/slice-job-nonce/v1`, `misaka-palw/exec-v2/token-state/v1` (amendment 1) |
| envelope | magic `PXE2`, version 2, subtypes `EXEC_TX` / `EXEC_SLICE` (exclusive), 16 KiB header cap, distinct keyed-BLAKE2b signing/content/payload/binding/id domains and ML-DSA-87 contexts per subtype |
| anchor trailer | `PXA2` v1 at the end of a **chain** block's coinbase extra data: up to 8 heads, count, Merkle root over the covered members in canonical order |
| limits | open roots 256 (4 per bond), extra executors 7, slices per root 128, pending depth 4 (16 per bond), 8 slices folded per block, lifetime 100,000 DAA, root work 2^48 |
| fence | `Params::palw_exec_payload_v2: Option<ForkActivation>`, mirrored by `sync_palw_exec_payload_v2()` into the V2 bundle; in `palw_fences_v1`, `for_each_fence` and both id hashers (Some-only) |
| arming prerequisites (enforced the day it flips) | ConsensusV2, `palw_execution_lane` open, `palw_lane_accept_parents_first`, `palw_rcore_plus`, `palw_canonical_work` and **`palw_probabilistic_constraints_v1`** (the verification route) in force at or below it; **`palw_audit_2026_09_11` declared** (tag 130's mixed-fleet tolerance) |
| armability | `PALW_EXEC_PAYLOAD_V2_ARMABLE = false`; `palw_exec_payload_v2_armable_on(params)` is true only on a salted testnet-12 drill (network testnet-12, genesis not public testnet-12's) |

## 3. Design as built (where it departs from or fixes the spec's open choices)

* **Carriage.** Lane edges are execution data. Past the fence a chain block names *no* lane block as a parent (header rule); lane blocks
  hang from one chain anchor plus lane parents. A chain block's coinbase carries the `PXA2` trailer; the covered set is the closure of
  its heads through lane-parent edges, stopping at what the parent state already anchored and dropping anything outside the two-span
  window, with a foreign anchor, or carrying a legacy `PXR1` envelope. The covered blocks are appended to the in-memory `mergeset_reds`
  of the anchoring block's ghostdag data only. The stored ghostdag data is untouched, so blue score, blue work, DAA, bits, pruning
  point and mergeset size cannot see an EXEC block, and the existing verdict / acceptance / coinbase / non-DAA code treats a covered
  block exactly as it treated a red round block. The same function builds the template's anchor and validates the walk.
* **v1 does not coexist.** Past the fence an algo-10 header carries exactly a `PXE2` envelope; a `PXR1` envelope is refused by name,
  and below the fence a `PXE2` header is refused by name. One carrier is never accepted through both paths. `PXR1` bytes keep their
  validation below the fence.
* **Accounting.** `root_credited + sum(credited disjoint slice work) <= canonical root claim work` (the root's prefix is the REAL claim's
  admitted canonical work), one accepted use per `(root, index)`, one job-work identity per live session, one settlement per root at
  the root claim's `Final`. Slices have no `Final`, subsidy or escrow of their own. Extra executors' exposure is the claim reservation
  in the one committed ledger and is re-derived in `assert_internal_consistency`.
* **Hold, expiry, void.** A licensed root claim whose session is not ready owes no `Final` deadline (the DA/credit pause); a session
  not ready by its expiry voids the claim uncharged (`WorkRootExpired`): silence is never a conviction.
* **Settlement.** The producer leg is split by work share (`sum paid == allocation`, nothing minted); slice legs go through
  `add_panel_payout` (unvested). Splitting a range into more slices pays no more.
* **Permit verdicts.** The covered `EXEC_TX` members are judged in canonical `(round, index, block hash)` order: the lowest hash takes
  a permit, a second block of the same permit is `PermitAlreadyUsed`, and the distinct payees one coinbase may pay are bounded
  (`PayeeBound`). A refused member is covered and credited nothing; it never makes the chain block invalid.
* **Reorg consequence (by design, documented).** The anchoring window is the block's span and the one before (a span is one DAA on
  testnet-12). A lane block anchored on a branch that is reorganised away, and outside the window on the new branch, is not covered
  there; the executor republishes. Nothing is invalidated or double credited (test 10 below).

## 4. Requirement -> code -> test -> status

Pipeline tests are `consensus/src/pipeline/virtual_processor/tests/t12_exec_v2_carriage.rs` (abbreviated `P#`); fold tests are
`consensus/core/src/palw_state_v2/tests/exec_v2_fold_v1.rs` (`F`); module tests live beside their module.

| spec | requirement | code | evidence | status |
| --- | --- | --- | --- | --- |
| 1 | EXEC never a selected parent; zero raw/PALW weight, blue score, DAA/retarget, pruning level, k-anticone | `palw_exec_v2_augment` (reds appended in memory only), header rule (no lane parent from a chain block), algo 10 derives no block level (`algo_id_derives_no_block_level`, unchanged) | P1 twin-chain comparison of blue score, blue work, DAA, bits, timestamp, pruning point, mergeset size and absence from stored mergesets; P9 14-block burst against a twin; P3 chain block naming a lane parent refused | IMPLEMENTED_AND_TESTED |
| 1 | heartbeat / BASE-0 / REAL unchanged; EXEC never feeds beacon entropy | no edit to those rules; the challenge contract tags a slice `ExecWorkSlice` (not a beacon source) | P1/P9 twins; `misaka-palw-sdk/tests/exec_v2_work_slice_subject.rs`; the contract's own eligibility test | IMPLEMENTED_AND_TESTED |
| 1 | fence dormant on all presets, fingerprints unchanged, refused until section 9 passes | `palw_exec_v2.rs` (`PALW_EXEC_PAYLOAD_V2_ARMABLE`, `palw_exec_payload_v2_armable_on`, `validate_palw_exec_payload_v2`), `params.rs`, `fork_id_v1.rs` | `tests/palw_exec_payload_v2_fence.rs` (7 tests: 20 rulesets dormant, `never()` collapses, a height moves ruleset+schedule not identity, every real height refused by name, mirror consistency, **a salted drill may arm and a public ruleset may not**) | IMPLEMENTED_AND_TESTED |
| 2 | exhaustive dispatch; both/neither/unknown version reject the whole carrier | `PalwExecV2Envelope::validate_shape`, `expected_payload_root` | `palw_exec_v2` module tests (`the_dispatch_table_is_exhaustive...`, `decode_is_strict`, `a_decodable_envelope_of_another_version...`) | IMPLEMENTED_AND_TESTED |
| 2 | distinct TX/SLICE domains; no cross-network/version/anchor/subtype replay; golden vectors | keyed domains + contexts, `signing_message` | `domains_are_distinct...`, `a_permit_signature_never_verifies_as_a_slice_signature...`, `golden_vectors_pin_the_hashes`, `subtype_bytes_are_pinned`; P2 forged commitment / re-hung anchor refused | IMPLEMENTED_AND_TESTED |
| 2 | constant algo-10 header price for both subtypes; no free header | no change to the lane's target; relay admission (amendment 1, §10.3) | P2 (a slice block is an algo-10 constant-target block); `palw_exec_v2_relay` tests | IMPLEMENTED_AND_TESTED; the flood residual decided (§10.3) |
| 3 | root on an accepted REAL claim; prefix = admitted canonical work; no whole-session work to the root | `apply_root_declared_v2` | F `a_root_opens_on_an_accepted_claim_and_earns_nothing`, `every_refused_declaration_is_named_and_writes_nothing`, `a_root_declaration_is_signed_by_the_claims_bond...`; P1 (opened through a signed 0x4b carrier, prefix == claim pwu) | IMPLEMENTED_AND_TESTED |
| 3 | plan is a partition with no gap or overlap | `palw_work_plan_range_v2` | `a_plan_is_a_partition_with_no_gap_and_no_overlap`; F `a_session_opens_once_and_a_job_is_worked_once` | IMPLEMENTED_AND_TESTED |
| 3 | six admission rules in order, refusal named, no write on refusal | `exec_v2_admit_slice_v1`, `apply_covered_slices_v2` | F `each_admission_rule_refuses_by_name_in_the_specs_order...`, `a_used_index_a_skip_an_overlap_and_a_replay_credit_nothing`, `a_correct_slice_borrowed_from_another_root_or_job_is_refused_by_binding`; P4 unauthorised / skipping / borrowed slice anchored and credited nothing | IMPLEMENTED_AND_TESTED |
| 3 | pending depth allows the next slice before verification; bounded | depth 4, per bond 16, 8 per block | F `the_pending_depth_and_the_per_bond_quota_bound...`, `two_slices_in_one_block_see_each_other_in_order...` | IMPLEMENTED_AND_TESTED |
| 3 | slice binds root/class/kernel/plan/job/range/predecessor/result/input/output/evidence/DA/executor/identity | `PalwWorkSliceV1::slice_id`, `challenge_binding` | `the_challenge_subject_is_a_pure_function_of_the_slice...`; sdk test: flipping any of the 14 fields moves the contract's `ChallengeSubjectV1::id()` | IMPLEMENTED_AND_TESTED |
| 4 | separate permit / work / root / job ledgers; branch-local; atomic restore | `PalwExecV2StateV1`, journaled `ExecV2Row` | F `two_branches_hold_independent_ledgers_and_each_credits_a_slice_once`, `the_exec_v2_tail_is_pinned_at_0xee...`, `a_corrupted_ledger_is_caught_by_the_consistency_check`; P10 reorg at the pipeline | IMPLEMENTED_AND_TESTED |
| 4 | checked ints; conservation; one settlement per root; no re-escrow; no carrier subsidy | `settle_root_v2`, `settle_root_partial_v2`, `settle_at_final_v2` (+ the leg cap, §10.5) | `settlement_conserves_the_allocation_exactly...`, `splitting_a_range_into_more_slices_pays_no_more`; F `the_root_settles_once_at_final...`; **F `amendment_1::a_final_kernel_claim_verifies_its_slice_and_the_root_settles_once_with_capped_legs`, `a_slice_leg_is_capped_at_what_the_route_still_holds_on_its_claims`** (verification through kernel claim rows, no test door) | IMPLEMENTED_AND_TESTED at fold level; the composed real-node run is section 12 |
| 4 | root Final held while not ready; expiry voids uncharged; rows retire with the claim | `final_is_held_v2`, `sweep_expired_roots_v2`, `on_claim_voided_v2`, `on_claim_retired_v2` | F `a_licensed_claim_with_an_unready_session_owes_no_final_deadline...`, `an_unfinished_session_voids_its_claim_uncharged_at_its_expiry`, `the_rows_retire_with_their_claim...` | IMPLEMENTED_AND_TESTED (fold) |
| 4 | EXEC_TX permit use unchanged; one consumption per key; fee accepted and paid once | `palw_round_verdicts_v1` (canonical order, `taken`, `payees`) | P6: permitted spend accepted, fee to the bond's registered payout once; duplicate permit and ungranted round covered and refused | IMPLEMENTED_AND_TESTED |
| 4 | one credit, no N+1 finalized attempts; slice work is not extra reward | slices create no attempt, no permit, no credit; settlement only re-divides the root claim's own producer leg by work share | F the session-free twin comparisons (producer leg + slice legs == the twin's leg; the root claim's Final is the only one) | IMPLEMENTED_AND_TESTED; the schedule-credit question decided: **no raise** (amendment 1, §10.4) |
| 5 | lane edges are execution data; a chain block names no lane parent; same classifier for both subtypes | header processor, `round_lane_members_v2` | P3 | IMPLEMENTED_AND_TESTED |
| 5 | header/body-committed head/closure commitment, canonical order with subtype, bounds, window | `palw_exec_v2_anchor.rs`, `check_exec_v2_shape`, `palw_exec_v2_anchor_verify` | 16 anchor module tests; P1, P3 (lying count/root disqualified, heartbeat still anchors), P2 | IMPLEMENTED_AND_TESTED |
| 5 | template filters stale heads, resumes from the lane checkpoint; invalid carrier skipped by verdict, not by chain fault | `palw_exec_v2_pick_heads`, `palw_exec_v2_virtual`, `exec_v2_slice_adapt_block_template`, `heartbeat_adapt_block_template` keeps the trailer | P1, P9 (health view), P10 (stale head skipped, republished block anchored) | IMPLEMENTED_AND_TESTED |
| 5 | sync, pruning-proof, IBD, template, orphan pool treat the lane alike | `services.rs` hooks, `SyncManager::with_exec_hooks`, `deps_manager.rs`, `orphans.rs::block_deps`, body-stage head dependency (`MissingParents`, retryable) | P8 (headers and bodies through both hooks; the un-hooked list lands every header and is refused the anchoring body by name), P7 (arrival order), `orphans::an_anchoring_block_waits_for_its_lane_heads_as_for_its_parents` | IMPLEMENTED_AND_TESTED |
| 5 | legacy v1 coexistence defined; no double acceptance | v1 refused past the fence (section 3) | P2, P5 | IMPLEMENTED_AND_TESTED |
| 6 | `WORK_SLICE` challenge under RFC-0007 Part VI; positive verification of the whole root | the slice's kernel-route claim is its verification (`palw_exec_v2_verify`, amendment 1 §10.1): admission binds it; `sync_slice_verification_v2` reads its Final after the route's tick; the root is ready only when every slice is verified; the `WORK_SLICE` subject binding stays on the slice | F `amendment_1::*` (binding refusals by name; Final verifies; Final at admission verifies at once); `palw_exec_v2_verify` module tests (token state, nonce) | IMPLEMENTED_AND_TESTED at fold level |
| 6 | public-bond prosecution of a slice or boundary; exact court; DA default; false Valid; localization | the kernel route's own (G14): an outside bond files `FileProof` / `FileDemand` on the slice's kernel claim from public rows and DA; a conviction or default reaches the slice through the sync | the route's real-node G14 cases (`g14_kernel_route_*`, `g14_opv_*`: covered lie convicted by an outsider pre- and post-Final, withheld material defaulted); F `amendment_1::a_convicted_slice_voids_*`, `a_defaulted_slice_voids_*` | IMPLEMENTED_AND_TESTED per component; one composed real-node run (REAL root on a kernel-bound class → kernel-claimed slices → outsider conviction) not built: section 12 |
| 6 | a proved false predecessor voids the dependent suffix; no prefix payment | `void_suffix_v2` (amendment 1, §10.2); void reasons 131/132, uncharged at V2 | F `a_convicted_slice_voids_its_suffix_and_the_root_and_charges_the_root_nothing`, `a_defaulted_slice_voids_*_and_a_verified_one_convicted_later_still_voids_an_unsettled_root` | IMPLEMENTED_AND_TESTED (fold) |
| 6 | EXEC_TX acceptance independent of a co-located session | verdicts are per member | P6 (a TX block is judged with no session state) | IMPLEMENTED_AND_TESTED |
| 7 | per-root/bond/lane quotas, closure walk bound, independent slice queue; deterministic under every arrival order | constants in section 2; closure leaf bound = lane `max_per_mergeset`; heads 8; relay admission and the producer's one-carrier-per-slice rule (§10.3) | F quota test; anchor `a_lane_longer_than_the_bound_is_refused...`, `heads_are_bounded_and_a_diamond_is_covered_once`; P7, P9; `palw_exec_v2_relay` tests; `palw_exec_slice_producer` decision-table test | IMPLEMENTED_AND_TESTED (relay and production node-local) |
| 7 | bounded dedup checkpoint after pruning | `anchored` window drop; rows retire with the claim; the carriage tail travels in pruned import | F `an_anchor_records_what_it_covered_once_and_the_window_drops_the_old`; carriage round trip | pipeline pruned import not run: EXTERNAL_GATE_PENDING (drill) |
| 8 | observability: subtype, pending/verified/voided, root state, evidence, backlog, the reason for a lane refusal | RPC op 240 `getPalwExecV2Status` (`palw_exec_v2_observation_v1`): roots, slices, each verification claim's state, a block's refusal record (delta note 181), lane health; refusal codes pinned | F `a_slice_whose_claim_is_final_at_admission_*_and_the_refusal_record_names_every_carrier`, `refusal_codes_are_distinct_and_named`; gRPC `op_240_round_trips_on_the_grpc_wire`; rpc-core mock round trips | IMPLEMENTED_AND_TESTED (read model); an explorer page reads op 240 (none written) |
| - | restart reopens the same database with the lane state intact | store tip + delta rows | P7 | IMPLEMENTED_AND_TESTED |
| - | reorg across slice acceptance | per-block state | P10 (node 1 reorganises onto a longer branch; credit exists on neither branch until republished; a third node agrees) | IMPLEMENTED_AND_TESTED |
| - | the operator's `EXEC_TX` producer signs `PXE2` from the fence (the v1 producer would stop the permit lane at it) | `kaspad/src/palw_round_producer.rs` (`signed_round_commitment`, `PalwRoundProducerConfig::exec_v2_fence` from `Params::palw_exec_payload_v2_fence()` in `daemon.rs`) | unit test `the_round_commitment_is_pxr1_below_the_fence_and_a_valid_pxe2_exec_tx_from_it`: PXR1 byte for byte below the fence; from it a `PXE2` envelope the header stage's stateless check accepts and a different nonce does not; the node-side half (template, verdicts, payout) is P6 | IMPLEMENTED_AND_TESTED at the signing unit; the running kaspad producer across a fence is a drill (EXTERNAL_GATE_PENDING) |
| - | an `EXEC_SLICE` producer | `kaspad/src/palw_exec_slice_producer.rs`: intents name `(root, index, kernel claim)`; the statement is derived from public rows (`palw_exec_v2_slice_statement_v1`); template → adapt → PoW → `PXE2` slice signature → submit; republish of a carrier that left the window uncovered (§10.7). The computation and its kernel claim are the kernel route's own producer path | `palw_exec_slice_producer` tests (decision table, intent parsing) | IMPLEMENTED_AND_TESTED at the unit; the running service across a fence is a drill |

## 5. Section 9 gates

| gate | evidence now | open | status |
| --- | --- | --- | --- |
| Root lifecycle / REAL eligibility | root opens on a really admitted REAL claim through the pipeline (P1); prefix is the claim's canonical work; session open earns nothing; the hold, expiry void; **verification by kernel claims, root `Final` and the one capped settlement at fold level without a test door** (F `amendment_1::*`) | one composed real-node run (section 12, item 1) | CODE (one E2E) + EXTERNAL (drill) |
| Wire / compatibility | golden vectors, domains, strict decode, fence below / at / above activation (P5, P2, **P11: unarmed and armed-far nodes agree byte for byte**), v1 refused past the fence, fingerprints unchanged, **registry re-audited at both integration merges** (section 11) | old-node behaviour against a v2 header across a real fence is a drill | IMPLEMENTED_AND_TESTED for the unit/pipeline part; drill EXTERNAL_GATE_PENDING |
| Lane isolation | twin-chain numeric equality, mergeset absence, forged headers refused, 14-block burst (P1, P2, P3, P9) | burst at lane width (hundreds), paired baseline/v2 DAG runs | IMPLEMENTED_AND_TESTED for the cases listed; capacity EXTERNAL_GATE_PENDING |
| Accounting | duplicate, overlap, skip, cross-root/job replay, checked overflow, repeated Final, prefix conservation, branch independence (F, module tests, P4, P9, P10); the leg cap; no schedule-credit raise (§10.4) | — | IMPLEMENTED_AND_TESTED |
| Public verification | the kernel route's G14 cases (outsider conviction pre/post Final, DA default, OPV windows) carry over by the binding; suffix void and both void reasons at fold level | the composed real-node run (section 12, item 1); the kernel route's own external soundness review | CODE (one E2E) + EXTERNAL |
| Liveness / capacity | relay admission and production backpressure built (node-local) | stopped producer / verifier / DA / court; no TX starvation under slice flood; identical heartbeat/BASE-0/clock/anchor-duty outcomes | EXTERNAL_GATE_PENDING |
| Deterministic state / recovery | restart (P7), shuffled arrival and head-before-body (P7, orphans), IBD through both lists (P8), reorg (P10), fold branch independence, P11 replays | pruned import and archival-vs-fresh-vs-pruned comparison, property tests over event histories, stale head wedge drill | partly IMPLEMENTED_AND_TESTED; drills EXTERNAL_GATE_PENDING |

## 6. Gaps, named (as X8 left them, with X8R's resolution)

1. **Verification route** — X8: none. **X8R: built** (amendment 1 §10.1): a slice is verified, convicted or defaulted only through its
   kernel-route claim; the binding refuses by name; the sync runs after the route's tick. The test door `mark_slice_verified_for_tests`
   remains for chains with no kernel route (the arithmetic tests). Pipeline-class slices are refused (`VerificationKindUnsupported`)
   until a segment state is defined for them.
2. **Post-Final liability of slice legs** — **decided and built** (§10.5): the kernel reservation is the liability; each leg is capped
   at it.
3. **Suffix void** — **built** (§10.2).
4. **Flood residual** — **decided** (§10.3) and the relay admission / production backpressure **built** (node-local).
5. **Anchoring-window strand** — **decided** (§10.7): the window stays two spans; the producer republishes.
6. **Schedule credit of settled slice work** — **decided** (§10.4): no raise in the first release.
7. **Repeated jobs** follow the existing ticket rules (unchanged).
8. **RPC, refusal record, producer, permit equivocation** — RPC op 240, delta note 181 and the `EXEC_SLICE` producer **built**; v2
   permit double-signing **decided** (§10.6): no offence (honest reattachment re-signs a permit), the canonical order makes a second
   block `PermitAlreadyUsed`, relay keeps it un-announced.
9. **The fork-id probe** — the integration line's own (unchanged here).

## 7. What arming still needs

There is no DAA-9,000 flag day (user, 2026-10-08): the fence rides the one full-activation release, after every RFC is implemented.
The code gates of this fence are closed except one (section 12, item 1). In order:

1. **Code:** the composed real-node E2E (section 12, item 1). **Owner dependency:** the kernel route (`palw_probabilistic_constraints_v1`)
   is a prerequisite and is itself refused by its own validation; the fence can be armed — on a drill or a release — only together with
   it.
2. **Drill path (done here):** `palw_exec_payload_v2_armable_on` lifts the gates' refusal on a salted testnet-12 drill; every other refusal
   applies, so a drill arms it only together with the kernel route, the OPV / onboarding fences its classes use, and the five structural
   prerequisites.
3. **Release:** flip `PALW_EXEC_PAYLOAD_V2_ARMABLE` in the change that sets the height, after review; add the height to the release's
   fence list and to `t12-repin.sh`; re-pin the goldens (a schedule entry moves the schedule id). Prerequisites at or below the height:
   ConsensusV2, execution lane, `palw_lane_accept_parents_first`, `palw_rcore_plus`, `palw_canonical_work`, `palw_probabilistic_constraints_v1`,
   and `palw_audit_2026_09_11` declared.
4. **EXTERNAL drills** (spec section 9), with the shipped binary — the operators' round producers sign `PXE2` from the fence and the slice
   producer publishes from intents: below / at / above activation including the `PXR1` cutoff (an old EXEC block in the window at the
   fence); a root opened before the fence (dropped by name); restart and pruned import across the fence; the two sync lists with an
   un-upgraded peer; a slice flood against the baseline DAG with equal heartbeat/BASE-0/clock/anchor-duty outcomes and no EXEC_TX
   starvation; a stopped verifier / DA / court run (slices stay pending, roots expire uncharged); an outsider convicting and defaulting a
   slice through the kernel route on the drill chain; a reorg across the fence; archival-vs-fresh-vs-pruned state comparison.

## 8. Tests and how to run them

```text
cargo test --offline -p kaspa-consensus-core --lib palw_exec          # envelope, anchor (module tests)
cargo test --offline -p kaspa-consensus-core --lib palw_work_slice_v2
cargo test --offline -p kaspa-consensus-core --lib exec_v2_fold_v1    # fold: declaration, admission, settlement, void, expiry, branches
cargo test --offline -p kaspa-consensus-core --test palw_exec_payload_v2_fence
cargo test --offline -p kaspa-consensus --lib t12_exec_v2             # 10 pipeline tests (P1..P10)
cargo test --offline -p kaspa-p2p-flows --lib orphans
cargo test --offline -p misaka-palw-sdk --test exec_v2_work_slice_subject
```

Regression sets run green with this lane in: `t12_round_lane_e2e` (5), `lane_accept_parents_first` (5), `ibd_parents_first` (1),
`kaspa-consensus-core --lib` (all but the two pre-existing fork-id probe failures above).

Pipeline tests: P1 `slices_are_carried_anchored_credited_and_weightless` - P2 `header_and_body_gates_refuse_each_hostile_lane_block_by_name` -
P3 `a_chain_block_names_no_lane_block_and_a_lying_trailer_is_disqualified_while_a_heartbeat_anchors` -
P4 `an_unauthorised_a_skipping_and_a_borrowed_slice_are_anchored_and_credited_nothing` -
P5 `below_the_fence_a_pxe2_header_is_refused_and_a_root_declaration_is_dropped` -
P6 `a_permitted_tx_block_is_anchored_its_fee_paid_once_and_a_duplicate_permit_or_an_ungranted_round_is_refused` -
P7 `lane_state_survives_a_restart_and_a_replaying_node_agrees_whatever_the_arrival_order` -
P8 `ibd_carries_the_anchored_lane_blocks_through_both_sync_lists` -
P9 `a_burst_of_competing_slice_blocks_is_anchored_once_credited_once_and_weightless` -
P10 `a_reorg_unanchors_the_lane_and_the_winning_branch_anchors_and_credits_it_once`.

## 10. X8R review of every pipeline path the fence does not guard (2026-10-08)

Lane X8R (branch `rfc8/x8r-review`) reviewed every change of `6176a636a..960f65a42` outside the fold. The bar, from the user's directive:
each change is either **(a) a no-op while `palw_exec_payload_v2` is `None`** — byte-identical verdicts, state and relay behaviour on
the live int-12 ruleset — or **(b) a deliberate node-only change that is safe for a mixed fleet**. A change that moved live behaviour with
the fence off is a bug and was gated or reverted. Because the real release will carry the fence armed at a height H while int-12
nodes are still on the network, every gate below is also checked **armed with H above the block** ("armed-below"): the gates read the
fence *at the block's own DAA*, not merely its presence, wherever a DAA is in hand.

One fact bounds several trailer findings: on every kaspa-pq preset the coinbase payload's miner script must be the 69-byte ML-DSA-87
P2PKH (`NonPqCoinbasePayloadScript`) and the payload cap is 204 bytes, so a payload is at least 88 bytes and a one-head `PXA2` trailer
(140 bytes) never fits: with the fence off a block whose tag ends in a trailer is always refused `PayloadLenAboveMax`. The trailer
readers below therefore could not change a *valid* block's verdict, but they did change what a node does with such a block before (or
instead of) refusing it, and one of them changed other miners' templates.

| # | path | X8 change | fence-off finding | verdict | X8R action / pin |
| --- | --- | --- | --- | --- | --- |
| 1 | `pow_layer0::check_palw_commitment_shape_at` (header isolation **and the pruning-proof header gate**) | a `PXE2` algo-10 payload gets its own 16 KiB cap and the v2 shape decode, ungated | relay path: still refused (later, at the stateless check, as `BadPalwCarriageAdmission` instead of `BadPalwCommitmentShape`; the refusal telemetry class moves). **Proof path: the proof gate runs only this check, so a pruning proof could carry a `PXE2` header that int-12 refuses** | **BUG** | new `check_palw_commitment_shape_exec_at(.., exec_v2_active)`; `_at` is the pre-X8 function exactly; the header stage and `PruningProofManager` (`with_palw_exec_v2`) pass the fence at the header's DAA. Pins: module test (`pre_v2` error equality), P5, P11 |
| 2 | `pre_ghostdag::check_round_lane_parents` | a non-lane block names no lane parent past the fence | gated `is_active(header.daa_score)` | (a) | — |
| 3 | `pre_ghostdag::palw_carriage_stateless_v2` | algo 10: `PXE2` below / `PXR1` past the fence refused by name | `PXR1` below the fence falls through untouched; a `PXE2` header no longer reaches this branch below the fence (row 1) | (a) | kept as defence in depth |
| 4 | `post_pow::round_lane_members_v2` (`check_mergeset_size_limit`, the header rule) | members decoded by the v2-aware `palw_exec_lane_coords_v1`; slices counted | identical for every `PXR1` byte string; `PXE2` cannot be stored below the fence after row 1 | (a) | made explicit: `lane_coords` is the v1 decode (same error text) where the fence is not armed |
| 5 | `post_pow::check_round_lane_mergeset` | envelope anchor == selected parent (v2); lane total bound `members + slices` before the v1 rule | with no slice the total bound is the v1 rule's first check, same error text (`TooMany`) | (a) | anchor check also gated on the fence being armed |
| 6 | `deps_manager::try_begin` | the anchor's heads are task dependencies, **ungated** | any coinbase ending in a trailer made the task wait for named pending blocks: no deadlock (each side commits to the other's hash, so no cycle), but a processing-order change a miner controls | **BUG** (minor) | gated on the fence at the block's DAA (`BlockTaskDependencyManager::new(fence)`) |
| 7 | `flowcontext/orphans.rs::block_deps` | the anchor's heads are orphan dependencies, **ungated** | an orphan whose tag ends in a trailer naming unknown hashes was **held for those hashes forever** (until eviction) where int-12 releases it with its parent and hands it to consensus: a relay divergence, and the sender of the invalid block is never charged | **BUG** | gated on the fence at the block's DAA (`OrphanBlocksPool::with_exec_v2`). Pin: `orphans::fence_off_a_trailer_in_a_miners_tag_is_not_a_dependency` (None and armed-above) |
| 8 | `processes/sync` hooks | IBD lists the anchored lane blocks and requests header-only lane children | hooks installed only where the fence is armed (`services.rs`): `None` is the old code path | (a) / (b) | armed-below: the children hook now lists only lane blocks at or past the fence, so the body list is int-12's below it |
| 9 | `body_validation_in_isolation::check_exec_v2_shape` | trailer shape, no trailer on a lane block, a slice carries only its coinbase | gated `is_active(daa)` | (a) | — |
| 10 | `body_validation_in_context` | heads with no body → retryable `MissingParents`; `check_payload_len_at` before the payload read | heads gated; inactive `check_payload_len_at` is `len <= cap` else `PayloadLenAboveMax(len, cap)` — the error the payload reader gave, at the same point | (a) | P11 pins the over-cap trailer refusal string |
| 11 | `coinbase::deserialize_coinbase_payload` | a payload ending in the magic may exceed the cap by a trailer, **ungated** | every body-validated payload passed the strict check first; a reader of a payload that skipped in-context validation (trusted blocks) would accept what int-12 refuses | **BUG** (latent) | lenient only where the fence is armed (`CoinbaseManager::with_exec_v2_armed`) |
| 12 | `coinbase::modify_coinbase_payload` | a trailer at the end of the cached template's payload survives the next miner's data, **ungated** | the bytes are the *previous miner's* tag (`getBlockTemplate` extra data): re-appended to another miner's payload they push it over the cap, so one RPC caller poisoned every other miner's template | **BUG** (node-local) | carried only where the fence is armed |
| 13 | `virtual_processor::palw_exec_v2_augment` | covered blocks appended to the in-memory reds | gated `is_active(header.daa_score)`; returns the same `Arc` otherwise | (a) | — |
| 14 | `palw_exec_v2_non_daa` (chain walk ×4, virtual, `utxo_validation`) | covered blocks join the non-DAA set | gated on `palw_exec_v2.is_none()` only: armed-below it extended the set with the mergeset's round blocks (idempotent — `difficulty.rs` already puts every round block outside the DAA set) | (a) | now gated on the block's own DAA |
| 15 | `palw_round_verdicts_v1` | canonical order `(round, index, hash)`, a running `taken` set and the payee bound, **ungated** | chain mergesets: the header rule makes both checks redundant and `uses` is re-sorted, but the `judged` order (telemetry) moved, and **virtual's mergeset is no header the rule judged**, so the checks could change virtual's UTXO view | **BUG** (node-local) | order and both checks only past the fence; below it the old hash order, byte for byte |
| 16 | `palw_record_round_verdicts`, `exec_lane_adapt_block_template` (the round template refactor) | coordinates through the v2-aware decoder; the extra total bound | identical for `PXR1`; total bound = the v1 rule's first check | (a) | — |
| 17 | `heartbeat_adapt_block_template` | the trailer is re-appended to the beat's miner data | provably a no-op (a trailer in miner data re-serialises to the same bytes, else the fallback), but proof by argument | (a) | gated on the fence at the template's DAA |
| 18 | `build_block_template` (virtual's anchor) | the template's miner data carries the virtual's anchor | gated `palw_exec_v2_active_at(virtual daa)` | (a) | — |
| 19 | `palw_add_round_parents` | virtual takes no round parent within `mergeset_size_limit + 1` DAA of the fence | `None`: untouched. **Armed: the v1 EXEC_TX lane stopped up to 181 DAA (~8 h on testnet-12) before the fence on upgraded nodes** | (b) | virtual's own DAA (round parents add none) computed only within one mergeset of the fence; none past it |
| 20 | `calculate_virtual_state` | `UtxoProcessingContext` built after the DAA window; the exec state read | no side effect moved; the read is gated on the virtual's DAA | (a) | — |
| 21 | acceptance walk: tag 130 dropped by name below the fence | | equals int-12's skip at extraction: rent ceiling 0, no carrier refund, no slot, not an H-1 carrier, the refund settle sequence unchanged (the drop happens before any state is read) | (a) | P5, P11 |
| 22 | **`palw_lifecycle_object_may_ride_v2`, tag 130** | the declaration's shape checked at ride time | int-12 cannot decode tag 130 and, testnet-12 declaring the audit fence, **tolerates** it (A-2). An upgraded node refused a malformed declaration in isolation, so **a block carrying it was invalid on upgraded nodes and valid on int-12: a consensus split with the fence off** | **BUG (critical)** | tag 130 rides unjudged; `validate_palw_lifecycle_tx` judges it exactly as an undecodable payload (`Ok` iff the ruleset tolerates); arming now requires `palw_audit_2026_09_11` declared. Pins: `exec_v2_fold_v1::a_tag_130_carrier_rides_exactly_as_an_undecodable_payload`, P11 (a malformed declaration is carried and the block stands) |
| 23 | fold (`palw_state_v2.rs`, `palw_exec_v2_fold.rs`) | deadline hold, arm guard, exposure rows, `settle_at_final_v2`, void/retire hooks, expiry sweep, root block, tail `0xEE`, delta 180 | each is a no-op on an empty table (`settle_at_final_v2` returns `(amount, [])` and writes nothing; the sweep returns on an empty map; root block and tail are Some-only), and nothing fills the table unless the fence is active | (a) | — |
| 24 | enums | `WorkRootExpired`, `PayeeBound`, `ExecWorkRootOpenedV2`, delta 180, new `RuleError`s | written only past the fence; `RuleError` is not serialised | (a) | `WorkRootExpired` now explicit 130 |
| 25 | kaspad round producer, RPC/kaspad names, params and fork id | | producer: `exec_v2_fence: None` signs `PXR1` byte for byte (unit test); names are display only; ids Some-only with the `never()` collapse | (a) / (b) | repin (section 11) |

**Pins added.** P11 `t12_exec_v2_fence_off_every_lane_object_is_judged_as_before_and_an_armed_node_agrees`: node A (fence unarmed)
carries a plain chain, then two tag-130 carriers (one malformed), a heartbeat whose tag ends in a well-formed trailer and its twin one byte
off the magic (both refused with the same `PayloadLenAboveMax(len, 204)`), and a `PXE2` header (refused with the pre-X8 shape error);
node B (unarmed) and node C (armed at 1,000,000) replay every block and agree on every status, every refusal, the sink, the PALW state
root and the virtual UTXO multiset — before the lane objects and after them. P5 now expects the pre-X8 shape refusal. Core:
the shape-gate module test and the tag-130 admission test. Flows: the fence-off orphan test.

**Cross-lane findings (not this lane's code; reported to the Lead).** The row-22 split class is not unique to tag 130. On the
integration line every object kind appended after int-12 whose may-ride arm refuses anything is a pre-fence split on testnet-12 for the
same reason (int-12 tolerates the undecodable payload): `KernelRouteV1` (110: unsigned or oversized), `KernelConstraintReceiptV1` (111:
unsigned), the onboarding objects 104–107 and 109 (unsigned), `SignedRegistrationV1` (108), `PanelBeaconProofV3` (120: oversized). The
row-1 class likewise applies to RFC-0009's `PFS4` arm of the same shape gate (int-12's gate has no `PFS4` arm), on the pruning-proof path.

## 11. Integration (X8R)

* **Merges.** `ad1f609ed` merged the integration line at `9ea89994b` (past `86dbc1fc2`); `fd9a7f22e` merged it again at `35a9ae1c8`,
  which carries the UNSCHED reset (`f9c263d3e`): no DAA-9,000 schedule, the shipped testnet-12 ids back to the live int-12 release's.
  Conflicts were both-added arms only (RFC-0010's void reasons, the G14 / RFC-0010 objects, deltas and tails beside this lane's; one
  registry row in the remaining-RFC matrix) and were resolved as the union, the integration line's blocks first.
* **Allocation audit** (after both merges, against the whole tree): object tags — 130 is the only one of 130–139 in use and no other
  lane uses the range; deltas — 180 and 181 are this lane's, no duplicate discriminant anywhere in `PalwDeltaEntryV2` (160–161 G14, 170–173
  RFC-0010); carriage tails — `0xEE` is unique (`0xEC` kernel route, `0xED` RFC-0010); void reasons — **`WorkRootExpired` was X8's
  implicit 11 and is now the explicit 130**, 131/132 added by amendment 1, beside RFC-0010's 120–122; RPC — op 240 (gRPC 1294/1295) in
  this lane's 240–249. The registry (`remaining-rfc-integration-matrix.md` section 2) records all of it.
* **The shipped ids.** No change of this branch moves a params, schedule or identity id (the fence is `None` on every preset, Some-only in
  every hasher); `scripts/t12-repin.sh --shipping --drift-only` is the check (its result is in section 8).

## 12. What remains (code), and the cross-lane finding

1. **One composed real-node E2E**: a REAL root claim on a kernel-bound V2 class → its slices backed by kernel-route claims on the same
   chain → an outside bond convicting one slice (and defaulting another) through the route → the suffix void and the root void, plus the
   honest twin reaching `Final` and the capped settlement. Every link is tested — the route's G14 cases on the real node, the binding,
   sync, suffix void and settlement at fold level, the carriage in P1–P11 — but not in one run. **Blocker:** the testnet-12 harness
   produces REAL attempts only on the genesis card classes, which are not TIR registrant classes and so cannot be kernel-bound (tag 106
   requires the registrant's TIR class with the kernel class's program); a harness for an attempt on an onboarded TIR class is the
   missing piece (shared with the onboarding lanes).
2. **Pipeline-class slices**: refused by name until the K2-at-scale lane defines a segment state for pipeline (K2-TIR-v3) claims.
3. **The initial boundary** (`initial_state_root`) is the root bond's declaration; its link to the REAL claim's verified output is not
   checked (every slice after it is verified as a continuation of it).
4. **Cross-lane (routed by the Lead to A2U, `fix/a2-uniform-new-tags`):** every lifecycle object kind appended after int-12 whose
   may-ride arm refuses anything (110, 111, 104–109, 120) splits a mixed testnet-12 fleet before its fence, as tag 130 did; the `PFS4` arm
   of the commitment shape gate is ungated on the pruning-proof path. This lane's tag-130 arm is the pattern A2U's kind→fence table
   generalises.
