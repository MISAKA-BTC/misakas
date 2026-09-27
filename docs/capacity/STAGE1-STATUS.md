# ADR-0160 stage 1 (rcore/cap-s1) — status

**Update (resumed 2026-09-27):** all five lanes are merged, each built and tested (weight b44b7900f,
escrow b042b3c68, liab 70399f19b + decision 1 f53bdae9c, verify 88288c299, shadow d5e0a3a88). Target dir
is now the shared `~/Downloads/MISAKA-wt-b/wt-int4-target`. In progress: the rho=1 state-diff harness and
the six invariants. The sections below the next one are the pause-time notes (kept for history).

## Where a later merge of rcore/int-5 (PALW_T12_POST_LAUNCH_FENCES_V2 at DAA 1,300) touches this branch

* **params lists.** cap-s1 adds `PALW_T12_CAPACITY_FENCES_V1` (+ `palw_t12_arm_capacity_fences_v1`,
  `palw_t12_capacity_params_v1`) right before `palw_t12_arm_post_launch_fences_v1`, and removed every
  capacity entry from `PALW_T12_POST_LAUNCH_FENCES_V1`. int-5's V2 list and its arming call sit in the
  same region (`palw_t12_params_with_registry_v1`): keep three lists — V1 (750), V2 (1,300), capacity
  (dormant). The capacity fences' prerequisites (strict-win, lane A) are V1 entries; none is on V2.
  The pins (`*_is_t12_only.rs` T12_RELEASE = dbbc9104…) move to int-5's fp 24e1aec3… in one re-pin.
* **step 4c voids / the floor-refusal retry (`palw_floor_refusal_retry`).** Interactions with F-E / F-L
  are listed in the final report and below (E-4's hold on a 4c void, AG-5 convictability, the retry's
  re-anchor keeping the claim live so no E-4 hold starts).
* **lock life (`palw_final_lock_life_retro`).** F-L's AS-1/AS-2 re-price the lock a counted `Valid`
  posts and the duty at bind; the lock-life fences date the lock row. They compose (price vs life);
  a retro re-dating of V02 locks does not read the capacity step. Re-run the liab suite's L-T4/L-T4b
  and v_t5 after the merge.


## Base and plan

* Branch `rcore/cap-s1` off the shipped DAA-750 release `c3dbaee3c` (worktree
  `~/Downloads/MISAKA-wt-b/wt-cap-s1`, target `~/Downloads/MISAKA-wt-b/wt-cap-int-target`, `nice -n 15`, `-j 4`).
* Merge order: weight (3aa4abec4) → escrow (d6a058249) → liab (deccda81b) → verify (20571f86d) →
  shadow (0e4654527); build + tests after each; then the four 17:50 decisions, the rho=1 state-diff
  harness over {floor, 8k, 2M} x {13k, 100k, 1M}, the six invariants, the batteries and clippy.
* Baseline measured before any merge (c3dbaee3c): `cargo test -p kaspa-consensus-core --lib` =
  2,851 passed, 0 failed, 10 ignored.

## Done (committed as the WIP merge of rcore/cap-weight — UNBUILT, UNTESTED)

The weight lane is merged with every conflict resolved by hand; **nothing has been compiled since the
merge** (the pause came before the first build). Do not build on this merge's sha as verified.

* Conflicts: params.rs (17 — field/list/hash insertions beside pptake2's fence: both kept; the three
  `post_launch_fence_arming_tests` hunks: the release's side), fork_id_v1.rs and ruleset_candidate.rs
  (both kept), t12_repin.py (the release's pins + the capweight pin), processor.rs `dns_reorg_outcome`
  (the release's gate), t12_post_launch_fences_combined.rs (the release's file, whole).
* **One tie rule.** F-W's copy is gone: `palw_deep_reorg_capacity_v1` + its unit test (fork authority),
  the processor's `palw_capacity_weight_cap` field and `palw_capacity_shallow_ghostdag_win_v1`, F-W's arm
  in `dns_reorg_outcome`. `PALW_CAPACITY_SHALLOW_REORG_{DAA,WALK}_V1` are aliases of strict-win's
  constants. New `palw_fork_authority_v2::PALW_REORG_SHALLOW_TIE_NEVER_LOWERS_SINK_DAA_V1 = true`, read
  by the processor's walk (`if CONST && candidate_daa < incumbent_daa`) and by
  `validate_palw_capacity_weight_cap_v1`, which refuses F-W on a build without the check.
* **Capacity fences out of the DAA-750 list.** F-W removed from `PALW_T12_POST_LAUNCH_FENCES_V1`; new
  `PALW_T12_CAPACITY_FENCES_V1` (dormant everywhere), `palw_t12_arm_capacity_fences_v1`,
  `palw_t12_capacity_params_v1(at)` (params.rs), and `config::drill::palw_drill_capacity_fences_at_v1`
  (not wired to kaspad). drill.rs's post-launch test now asserts F-W stays dormant under the 750 drill.
* hb_fork_choice_probe.rs: the lane's `duel_release_set(at, cap)` arms the capacity list separately;
  the release's `ratchet` / `a_shallow_tie_never_lowers_the_sink_so_chained_releases_stop_at_two` gained a
  third case "release set + F-W".

## Open on the weight merge (do these first on resume)

1. Build `kaspa-consensus-core` and `kaspa-consensus` (tests too) and fix compile errors.
2. Flip the lane's pinned split assertions now that strict-win converges (commit 3d9d6d3ba's item 3):
   `capacity_probe_honest_sibling_forks_converge_past_the_cap` (strict-win arm: ties agree, exchange (2)
   is not a tie without F-W and may split), `capacity_probe_the_release_set_converges_…` (`!cap` arm:
   same), `capacity_probe_a_shallow_tie_is_ghostdags_…` (strict-win alone now flips at k ≤ 2).
3. Re-check `palw_capacity_weight_cap_is_t12_only.rs` (the lane's pin test) against the 750 release:
   it may read testnet-12 as launched; it must pin `dbbc9104…` / `7c652212…` with F-W dormant.
4. Add a unit test for `palw_drill_capacity_fences_at_v1` and for the capacity list (every entry
   dormant on every preset; armed at one height validates over the 750 fences).
5. Run consensus-core lib + the touched integration tests + the processor probes; commit.

## Next lanes (not started)

escrow → liab → verify → shadow. Each lane added its fence to `PALW_T12_POST_LAUNCH_FENCES_V1`; move each
entry to `PALW_T12_CAPACITY_FENCES_V1` at its merge (F-E, F-L — whose t12 schedule should become stage 1's
rho = 1 — F-R, F-B). The escrow module lists the four rewiring lines for the liab merge
(`capacity_escrow_credit_at_v1` → liab's step, `palw_escrow_credit_applies_v1` →
`palw_seat_credit_applies_v1`, the conviction floor → Tier(min(10%·C_min, 3E)), the bind guard).
Decision 1 is NOT on the liab lane yet (`palw_offence_is_intent_class_v1(DaDefault)` is `true` there;
must become `false`).

## Not started

The four decisions pass, the rho=1 state-diff harness, the six invariant property tests, the full
batteries and clippy, the floor-retry interaction notes (rcore/f2-floor-retry, step 4c vs E-4 / AG-5).
