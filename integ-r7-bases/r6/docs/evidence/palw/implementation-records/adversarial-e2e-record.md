# Adversarial E2E record — Agent C4 (round 1)

Branch `adv/c4-e2e` (base `717064b16`, plus the Lead's cherry-picks `098ffc749`, `fffaf74ec`, `3d27e026d`, `592ab3382`). Independent of the lanes: every
case below is a new test file, none weakens an existing assertion, no consensus parameter / fingerprint / wire / tag is changed by C4. The first
(only) source change C4 proposed — checked tensor-shape arithmetic — was superseded by the Lead's `098ffc749` (C4's own version was reverted first).

**Top invariant (G14).** Producer + every Panel seat collude; one ordinary public bonded verifier outside the Panel, from canonical public
authenticated material only, reaches objective conviction or a correctly classified DA/default. Plus: relays are never truth authorities, DA
providers never computation authorities, RPC agreement is never a proof, beacons never authorize semantics, heartbeat/BASE-0/EXEC/block hashes
never entropy.

**Level.** Everything here is the **reference level** (the in-process `KernelLedgerV1` fed the strict wire form, the pure contract/tool/library
functions, the client crates). The real-node G14 wiring (lane D phase 2) does not exist yet: every "real node" cell of the G14 matrix is GAP and
is the subject of round 2.

Verdicts: **PASS** (attack refused / correctly classified), **FAIL** (a defect, with the failing test), **GAP** (not expressible yet, with the missing
piece), **OBS** (an observation: behaviour that is by design or economic, recorded so nobody is surprised). A FAIL test is `#[ignore = "FAIL F-C4-n …"]`
and asserts the SAFE property: run it with `-- --ignored` to see it fail. "Fixed" means the Lead's commit made the (un-ignored) test green.

## How to run

```text
export CARGO_BUILD_JOBS=3 CARGO_INCREMENTAL=0
cargo test --offline -p misaka-palw-kernel    --test adv_c4_route            # 27 tests, ~75 s debug  (C4_EXHAUSTIVE=1: all 385 per-node lies, ~340 s; default every 4th)
cargo test --offline -p misaka-palw-kernel    --test adv_c4_fuzz             # ~10.6k mutants + 102,441 every-byte flips of RegisterClass (~100 s)
cargo test --offline -p misaka-palw-kernel    --test adv_c4_fuzz_pipeline    # ~9.2k mutants, 18,969 flips, one-claim, edge/stage lies
cargo test --offline -p misaka-palw-challenge --test adv_c4_contract         # 18 pass, 3 ignored FAIL
CARGO_PROFILE_RELEASE_LTO=off cargo test --offline --release -p misaka-palw-sdk --test adv_c4_conformance   # ~160 s (1,070 evidence flips re-executed)
cargo test --offline -p misaka-palw-remote --test adv_c4_proof --test adv_c4_registration --test adv_c4_transport --test adv_c4_redemption
cargo test ... -- --ignored        # the proven defects still failing
```

## Findings

| id | severity | what | test | status |
|---|---|---|---|---|
| F-C4-01 | P0 | a stranger with one bond panics the kernel-route fold with a tiny object: unchecked products over attacker-chosen tensor shapes (`element_count`, `Tensor::from_le_bytes` `n*width`, `LayoutV1::of` incl. `m*n`), reachable by `FileProof` (Decode logits, Kernel scalar openings), `Respond`; `overflow-checks = true` in release makes it a panic | `adv_c4_fuzz` (3 sites, 113 of 4,982 mutants) | **fixed `098ffc749`**; 0 panics over 10.6k + 9.2k mutants and ~121k byte flips now |
| F-C4-02 | MED-HIGH | a producer's Sybil opens a post-Final demand and stays silent: the forfeit (half to the Sybil, half burned) leaves nothing reserved, and the honest outsider's valid in-horizon proof was dismissed with a fee (effective horizon = `court_deadline_daa`; colluders recoup the accuser share) | `a_post_final_sybil_demand_default_must_not_erase_the_liability_of_a_lying_final_claim` | **fixed `fffaf74ec`** (post-Final forfeit burned whole; a Final claim inside its horizon is adjudicated with nothing reserved) |
| F-C4-03 | HIGH | any other bond re-signs a published claim's evidence for the same job and is paid again (no producer binding, no one-claim-per-job): 2x `claim_reward` for one computation | `a_bond_that_copies_a_published_claim_must_not_be_paid_for_the_same_job_twice` | **fixed `fffaf74ec`** (one claim per job) **+ `3d27e026d`** (seal, then reveal) |
| F-C4-04 | MED (lane D mapping hazard) | a conviction instructed `SlashFraud(reserved)` larger than the collateral the consumer had re-synced lower | `a_conviction_never_instructs_more_than_the_synced_collateral` (+ a two-claim bond cut below the first reservation) | **fixed `fffaf74ec`** (clamp + explicit `ReleaseClaim`); accountant balanced |
| F-C4-09 | MED (DoS) | opened by the first one-claim-per-job fix: a free, shape-correct but unbacked (junk-commitment) claim held the job for the whole check window against the honest producer, at no cost, repeatable with a second bond | `an_unbacked_claim_must_not_be_able_to_lock_a_job_against_the_honest_producer` | **fixed `592ab3382`** (a claim holds its job from the Panel's coverage, not from its commit) |
| F-C4-05 | LOW-MED | `challenge_seed_v1` mints a seed from ANY `WorkBeaconV1` (another epoch's, or an invented output/anchor); only callers that ran `verify_work_beacon_v1` are protected | `adv_c4_contract::a_seed_must_not_be_minted_from_a_beacon_that_is_not_this_contexts` | **OPEN** (ignored); a typed `VerifiedBeacon` returned by the verifier would make misuse unrepresentable |
| F-C4-06 | LOW | `collect_work_beacon_v1` sorts stably on `(settlement, occurrence, canonical_work_id)`: events tying on all three are ordered by arrival (execution commitment / profile not in the key) | `events_tying_on_the_whole_sort_key_must_not_be_ordered_by_arrival` | **OPEN** |
| F-C4-08 | LOW | `verify_conformance_evidence_v1` never reads `failures`: `Passed` evidence that lists failures passes | `passed_evidence_that_lists_failures_must_not_pass` | **OPEN** |
| F-C4-10 | MED | `state_root_of_pinned_header_v1`: the header hash commits `palw_state_root` only for `pow_algo_id` 6,7,8,9 (and only when non-zero); for every other algorithm the root is hash-invisible, so a hostile node can return the pinned block's header with ANY state root and bond/class/claim proofs verify as "PROVEN against pinned block" over the attacker's state (e.g. a client pinned at a legacy/genesis block) | `adv_c4_proof::a_state_root_edited_in_a_header_that_still_hashes_to_the_pin_must_be_refused_for_every_algorithm` | **OPEN**; fix: refuse (`HeaderCommitsNoState`) when the header's algorithm does not hash the root |
| F-C4-11 | MED | beacon-conformance scope flags `require_independent` / `require_backend` are the candidate's own commit-time choice (`--no-require-independent/--no-require-backend`): a reference-only run (`ImplSet{exec:false,ref2:false}`) is `Passed` 30/30 and a verifier mirroring the weak set returns `Verdict::Pass` (a full-set verifier correctly says `EVIDENCE_NOT_REPRODUCED`) | `adv_c4_conformance::a_scope_that_switches_off_the_independent_and_backend_implementations_cannot_pass` | **OPEN**; the flags must come from the approved soundness policy; `Pass` should carry the implementation set |
| F-C4-12 | MED | `verify_signed_registration_v1` reads class/root/owner from the PAYLOAD but never compares them with the file's displayed `class_id`/`artifact_root`/`owner_bond`/`object_digest`; `model submit` (`operator/model_bundle.rs:453`) feeds `s.class_id`/`s.artifact_root` to the registry verdict, the sent-journal path and duplicate detection. A relay editing the file's class to an already-registered one makes `model submit` print "already registered — nothing is sent" and never send the user's registration; filing under another class defeats duplicate detection. Only with explicit `--expect-*` (from-file mode is self-consistent) | `adv_c4_registration::a_signed_registration_whose_displayed_class_root_or_owner_is_not_its_carriers_must_be_refused` | **OPEN**; bind the four fields to the payload in verify, or use `VerifiedSignedV1` in the CLI |

## Observations and residual risks (no defect, recorded)

| id | what |
|---|---|
| O-C4-05 | `RegisterClass` / `PostJob` take no fee at the ledger; a zero-collateral synced bond registers 32 variant classes (distinct `max_positions`) and unlimited jobs; state growth is priced only by the consumer's carrier fee/mass |
| O-C4-06 | the block adjudication budget can be spent by structurally malformed `CommitClaim`s from zero-collateral bonds (charged before the collateral check, no ledger fee): the honest `FileProof` behind them in the same block is `OverBudget` (dropped, state untouched, no fee) and convicts next block |
| R-C4-10 | same-PROMPT copy: a bond posts its own free job with an identical prompt and answers it with the published trace (accepted, covered, 2x reward). Lead: consumer economics (job price >= reward) — not a kernel bug |
| O-C4-09 | a post-Final conviction frees the job and it is paid a second time (the fraud's reward is not clawed back, only the collateral is slashed). Consumer economics |
| R-C4-12 | commit order is not priority: of two correct claims the Panel's coverage order decides the holder; a later copy covered first wins. Inherent unless the squat is re-opened |
| O-C4-13 | lazy seal expiry: objects apply before the tick, so a seal one block past its ttl is still honoured |
| O-C4-14 | an unsealed structurally valid `CommitClaim` spends one adjudication before the seal check (put the seal check first) |
| O-C4-07 | `canonical_work_id` must be domain-separated by the consumer: an earlier non-real event sharing a real work's id suppresses it ("first occurrence counts") |
| O-C4-08 | the policy fixes no minimum `k` (a one-source beacon validates); approval is exact |
| O-C4-11 | the beacon needs `k` distinct work IDS, not `k` distinct executions |
| O-C4-15 | the signer pins class/root/owner only; the builder-chosen `activation_daa`, `share_permille`, `slash_value_per_pwu`, `initial_target`, `pwu_rule` are signed as given (consensus drops the invalid ones: fee lost; a wrong `activation_daa` inside the lookahead just delays the class) |
| O-C4-16 | the multi-RPC quote takes the MAX carrier fee: one lying node of three raises the quoted fee to anything under the user's cap |
| O-C4-17 | `model submit`'s OK row "registered … accepted by a quorum" omits the `UNVERIFIED_REMOTE_STATE` label (the preceding `state` line carries it) |
| G-EXPIRY / G-RULESET | confirmed (known): `expiry_daa`, `quote_digest`, funding maturity fields and `ruleset_id` are editable in the signed file without failing verification |

## Case table — kernel route ledger (reference level)

| attack | test | expected (stated before the run) | verdict |
|---|---|---|---|
| A producer + every Panel seat collude (pre-Final, post-Final, past horizon) | `adv_c4_route::a_producer_and_every_panel_seat_colluding_lose_to_one_ordinary_bond_with_a_fresh_verifier` | one outside bond, replaying the ENCODED blocks, convicts; reward stays paid post-Final; past the horizon dismissed | PASS |
| A' Sybil demand + silence, pre-Final | `a_pre_final_sybil_demand_default_cannot_stop_a_fast_outsider_and_never_yields_a_reward` | fast outsider convicts; slow outsider loses the conviction but the fraud earns nothing | PASS (OBS: a slow outsider cannot convict) |
| A'' Sybil demand + silence, post-Final | F-C4-02 | in-horizon proof still convicts | FAIL -> fixed |
| B self-consistent garbage; C borrowed trace; G input substitution; H last / mid-stream token substitution | `garbage_borrowed_and_substituted_traces_are_all_refused_or_convicted` | refused at inclusion or convicted | PASS |
| D / E / F one fault at EVERY node of position 1 (arithmetic, routing/TopK, history/state, quantize, gather …) | `a_single_lie_at_any_node_of_position_one_is_refused_at_inclusion_or_convicted_and_never_survives_clean` | committed or refused at inclusion, never found clean, convicted with 1000/500; the same proof against the honest claim is dismissed | PASS (385 of 385 with `C4_EXHAUSTIVE=1`) |
| I pipeline edge / stage fault | `adv_c4_fuzz_pipeline::a_pipeline_edge_lie_and_a_stage_lie_are_convicted_and_do_not_touch_the_honest_claim` + lane B's `k2_ledger_pipeline` | convicted; honest claim untouched | PASS (edge + stage; VLM/decode only by lane B) |
| J held / fused terminal | — | the ledger always registers with `ProfileMaterialV1::kernel_route(true)`; held/fused is reachable only through the gate | GAP at ledger level (gate-level in lane B's `k2_public`) |
| K malformed Merkle opening | `adv_c4_fuzz` opening mutants (axis, index, dtype, shape, siblings, values) | dismissed / refused, never a panic, never a conviction of the honest claim | PASS |
| L correct root, material withheld; M fake material | `withheld_material_fake_material_and_a_third_party_server_are_classified_and_never_convict_by_themselves` | transposed shape, other dtype, one element, a row where the whole is owed, another node's tensor, garbage, empty: each rejected with its class; silence defaults (penalty, not slash); a third party's true bytes serve | PASS |
| N court pre-emption; O simultaneous challengers | `court_pre_emption_and_simultaneous_challengers_convict_once_and_pay_once` | one conviction, one reward, two `Duplicate`, 6 sessions moot, bonds return | PASS |
| P challenge at the last block before Final | `a_proof_at_the_last_block_before_final_wins_and_one_block_after_is_a_post_final_conviction` | proof applied before the tick: never Final, no reward issued | PASS |
| Q invalid-challenge spam | `invalid_challenge_spam_is_priced_bounded_and_changes_nothing` + O-C4-06 | 64 dismissed (fee) / 236 over budget, honest lifecycle untouched | PASS |
| R restart mid-prosecution; T duplicate proof after replay | `restart_at_every_block_boundary_mid_prosecution_reaches_the_same_roots_and_a_replayed_proof_is_a_duplicate` | same root after every block from any restart point; replay is `Duplicate` | PASS |
| U bond exit while liability exists | `a_bond_cannot_leave_while_a_claim_or_a_demand_holds_a_reservation` | no withdraw while reserved (claim or demand), still convictable in the horizon, exact withdraw after | PASS |
| seal, then reveal (copyist seals after the reveal; guess seal; re-seal; ttl; exit; wrong signer; unknown job; 3,000 junk seals) | `a_copyist_that_seals_after_seeing_the_reveal_cannot_commit_before_the_original`, `seal_re_seal_expiry_exit_and_signer_edges` | the copy always commits later; the replaced seal opens nothing; exiting bonds neither seal nor reveal; seals swept at ttl+1 | PASS (O-C4-13, O-C4-14) |
| coverage races (two correct claims; refused coverage retried after the holder fails; squatter freed by a demand default) | `observation_the_panel_s_coverage_order_not_the_commit_order_decides_the_holder`, `a_refused_coverage_can_be_retried_after_the_holder_fails`, `a_panel_covered_squatter_is_freed_by_a_demand_default_and_the_honest_producer_then_finalizes` | the first covered holds; retry works; a covered squatter is breakable for a profit | PASS (R-C4-12) |
| codec: signer/actor mismatch | `an_object_signed_by_anyone_but_the_actor_it_names_is_unauthorized_and_changes_nothing` | every named-actor object by any other bond / unknown digest: `Unauthorized`, root unchanged | PASS |
| codec: non-canonical (reordered / duplicated map, Option tag 2, wrong version), tag confusion (every sample under every tag 0..15), length-prefix lies, ceilings of every tag | `non_canonical_encodings_length_lies_and_retagged_objects_are_refused_or_canonical`, `the_decoder_bounds_every_object_before_parsing_and_never_trusts_a_length_prefix` | refused, or equal to its own canonical encoding; never a panic | PASS |
| settlement conservation | `support::Book` (independent) on every block of every test and on every applied fuzz mutant | slash == reward + burn per block; value conserved except `FinalReward` / consumer `sync_bond`; the ledger's bond view == the book | PASS |
| wire fuzz, all 12 route variants + pipeline variants | `adv_c4_fuzz`, `adv_c4_fuzz_pipeline` | I1 no panic, I2 refusal leaves root, I3 honest never convicted, I4 conservation | PASS after F-C4-01 |

## Case table — contract, tool, client crates

| attack | test | verdict |
|---|---|---|
| heartbeat / BASE-0 / EXEC_TX / EXEC slice / receipt-only / provisional / Panel receipt as beacon | `adv_c4_contract::heartbeat_base0_exec_and_receipts_are_never_entropy_…` (adding all 7 kinds at 7 positions, any order, leaves the beacon identical; only-non-real = `Unavailable`) | PASS |
| unfinalized, DA-unsatisfied, validity-dependent, stale-committed, reattached, candidate / dependent work | `unfinalized_stale_reattached_and_self_work_never_become_sources_and_are_never_rescued` | PASS (O-C4-07) |
| candidate self-beacon with a forgetful consumer | `a_forgetful_consumer_cannot_let_a_conformance_candidate_seed_its_own_challenge` | PASS for MODEL/KERNEL conformance (`candidate_profile_id` required, candidate excluded even if the list is empty); **GAP-C4-A** for CLAIM_VERIFICATION / WORK_SLICE / PUBLIC_PROSECUTION: self-exclusion is the consumer's `excluded_profiles` |
| Panel-licensed source into PANEL_ASSIGNMENT | `a_panel_licensed_final_never_seeds_a_panel_assignment` | PASS |
| duplicate / reattached work; contribution reorder | `every_permutation_of_the_same_events_gives_the_same_beacon` (720 permutations) | PASS (F-C4-06 for exact key ties) |
| reorg before / after lock | `a_reorg_before_the_lock_changes_the_beacon_and_one_after_the_k_th_source_does_not` | PASS |
| forged / edited presented beacon (every source field, accumulators, output, anchor, lock, order, duplicate, missing) | `every_single_field_edit_of_a_presented_beacon_is_refused` | PASS |
| seed reuse across subject kinds / subject fields | `six_subject_kinds_six_seeds_and_no_single_subject_field_is_a_free_edit` | PASS |
| sampler bias (fixed seed, generous bounds) | `the_index_sampler_is_uniform_…` (chi-square n=3,10,1000; n=2^63+1 upper half 0.48–0.52), `the_distinct_sampler_…` (all C(10,3) subsets, 0.28–0.32), `the_field_sampler_…` (never p, top bit ~50 %, bound => error) | PASS |
| transcript round substitution (staged / Fiat–Shamir) | `transcript_rounds_cannot_be_substituted_reordered_or_rebound` | PASS; **GAP-C4-B**: `FiatShamirTranscriptV1::verify` does not know the expected statement/policy/beacon — the consumer must compare the header |
| extreme positions / counts never panic | `extreme_positions_windows_and_counts_never_panic_the_beacon` (≈ 650 contexts) | PASS |
| onboarding lifecycle shortcuts | `the_onboarding_lifecycle_has_no_shortcut_to_active` (model-based) | PASS; **GAP-C4-C**: failed conformance re-committed without limit (60 failures, then pass) — no failure counter / retry limit in the record |
| conformance evidence forgery (20 single-field forgeries, SKIPPED/INCOMPLETE/FAILED/UNAVAILABLE with passing counters, stale seed, other commitment) | `conformance_evidence_is_recomputed_…` | PASS (F-C4-08) |
| beacon-conformance tool: every single-byte flip of valid evidence, with re-execution | `adv_c4_conformance::every_single_byte_flip_of_valid_evidence_is_never_a_pass` (1,070 bytes) | PASS |
| stale / short / other-history facts (tip before lock, extra earlier source, changed execution commitment, dropped source, moved epoch / commitment position) | `stale_short_or_different_facts_never_verify_evidence_…` | PASS |
| facts-file fuzz | `mutated_facts_files_never_panic_…` (618 parsing mutants) | PASS: only `final_path.panel_seed_id/panel_epoch` and `tip_position` are ignorable |
| evidence of one commitment under another | `evidence_of_one_commitment_never_verifies_under_another` | PASS |
| artifact / layout / plan / policy substitution after commit; `--no-rerun`; SKIPPED-as-PASS | lane C's `runtime_pack_beacon` (not re-run) | covered by lane C |
| candidate-chosen scope strength | F-C4-11; `gap_a_tiny_scope_with_a_declared_total_fault_density_buys_forty_bits` (GAP-C4-D: 30 x (1 vector of 1 token + 1 leaf) derives >= 43 bits); O-C4-11 | FAIL / GAP |
| state-proof forgery: edited header (every algorithm), preimage byte, label, row value / key, truncated / dropped / duplicated / reordered / extra row, hidden row proving absence, other state under our pin | `adv_c4_proof` (4 tests) | PASS except F-C4-10 |
| state-proof file form fuzz | `the_state_proof_file_form_never_panics_on_garbage` | PASS |
| detached registration: any edit of any JSON leaf (79) or carrier bit (17,380 flips) of a signed registration | `a_relay_that_edits_any_field_or_any_carrier_byte_of_a_signed_registration_changes_nothing_or_is_refused` | PASS for the carrier; edits of displayed fields accepted (F-C4-12), unsigned `expiry_daa` etc. (G-EXPIRY) |
| unsigned bundle: 196 leaf edits then sign | `observation_the_signer_signs_whatever_object_fields_…` | a builder can change the funding outpoint and the unpinned object fields (O-C4-15); class / root / owner / fee / recipient never |
| multi-RPC disagreement | `quote_agreement_stops_on_any_identity_difference_and_merges_the_rest_to_the_worst_case` (13 identity fields) | PASS (O-C4-16) |
| duplicate submission vs resend | `resend_versus_duplicate_matches_an_independent_oracle_over_every_combination` (3 verdicts x 8 journals) | PASS |
| malicious relay replies; hostile providers (flipped / swapped / truncated / padded chunks, wrong-claim manifests, silence); hostile redemption listing (traversal, wrong length, empty); hostile HTTP peer (huge Content-Length, endless body, unterminated header, absurd chunked size, garbage, redirect) | `adv_c4_transport` (3 tests) | PASS |
| redemption: duplicate quantum range edges, fee at / above the cap, `u16::MAX`, empty / inverted range, other network / version / beacon rule, every field edited after signing, every byte flip of an `RDA4` bundle, split conservation over 110 (part, bps) pairs | `adv_c4_redemption` (3 tests) | PASS |

## GAP — not expressible yet (round 2 and later)

| # | what is missing |
|---|---|
| 1 | every **real-node** G14 cell: `KernelRouteObjectV2`-style carrier + acceptance arms + fold + per-block tick + bonds mapped onto `PalwBondKeyV2` + RPC reads (claim record, served positions, demands, verdicts, public material). Round 2 re-runs this record's kernel cases from a fresh verifier process against `T12Chain`, plus restart / IBD / reorg equality of the kernel-route root |
| 2 | real-processor redemption cases (payout to a non-registered destination, two builders racing, reorg after payout, V3 unchanged pre-fence): lane D's `rfc9_redemption_v4` exists; C4 re-ran only the pure primitives |
| 3 | real-node registration attacks beyond lane D's phase 1 (it is already broad: parity table, replay, restart, IBD, reorg) |
| 4 | EXEC work slices (RFC-0008 v2): no code on HEAD |
| 5 | the RPC wire (`getPalwStateProof` op 202, the registration reads) over a running service — exercised only through their functions |
| 6 | RFC-0010 Panel assignment, RFC-0001 and the VM lanes: not integrated |
| 7 | J held / fused terminals at ledger level; VLM / decode pipeline semantic attacks (lane B only) |

## Commits

`dfca3469d` fuzz (F-C4-01 reproduction) · `254018073` route E2E · `f49c97e4c` widened fuzz, pipeline, every-byte flips · `e655de052` re-verification of F-C4-02/03/04 and
the squat · `a310c9c3c` challenge contract · `5093f4446` seal / coverage · `35d702c93`, `7ea602cc0` conformance tool, proof forgery, registration, transport, redemption · this record.

---

# Round 3 — Agent C4r3, the REAL node (2026-10-08)

Branch `adv/c4r3-real-node` (base `183a761bf`, the Lead's integration HEAD). Rounds 1–2 attacked the reference crates (above; their
body is copied from `adv/c4-e2e@21b83fab2`). Round 3 attacks what landed on the node since: the kernel route in `PalwChainStateV2`
(tags 110/111, `SealClaim`, proof grace, post-Final liability, block budget, RPC 210–212), RFC-0015 OPV (Panel = 0), the onboarding
objects (104–108), and reads the RFC-0010 V3 fold and the single challenge contract. Every PoC runs the real path: signed `0x4b`
carrier → mempool → the node's template → chain-block fold → read API, in `consensus/src/pipeline/virtual_processor/tests/
g14_kernel_route_e2e/c4r3.rs` (a child of the lane's harness, `g14_c4r3_*`) and `consensus/core/tests/c4r3_fork_id_at_int13.rs`.
Convention as in round 1: a defect's test is `#[ignore = "FAIL F-C4R3-nn …"]` and asserts the SAFE property; `observation` tests pin
by-design or already-named behaviour, quantified.

**Nothing here touches what DAA 9,000 arms.** The int-13 list (`PALW_T12_INT13_FENCES_V1`: `palw_audit_1004_v1`,
`palw_gen_range_twin_v1`, `palw_model_court_window`, `palw_receipt_spend_v4`) arms none of the G14 fences; every G14 object is dropped
by name, first and charged nothing, below `palw_probabilistic_constraints_v1` (the acceptance walk), the G14 carriage tails are written
only when their state exists, and the only consensus reads of `consensus_params_id` are in dormant G14 rules (F-C4R3-01). F-C4R3-04
is a release-process hazard OF the 9,000 height.

## How to run

```text
export CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0
cargo test --offline -p kaspa-consensus --lib g14_c4r3                    # the passing PoCs, controls and observations
cargo test --offline -p kaspa-consensus --lib g14_c4r3 -- --ignored       # the proven defects (fail on this revision)
cargo test --offline -p kaspa-consensus-core --test c4r3_fork_id_at_int13
```

## Findings

| id | sev | what | PoC | root cause | status |
|---|---|---|---|---|---|
| F-C4R3-01 | **P0 where armed** (latent: the fences are refused at every real height) | **Consensus rules that bind `Params::consensus_params_id` split upgraded from un-upgraded nodes at the binary swap, not at any height.** `consensus_params_id` hashes every SCHEDULED fence (the int-13 re-pin moved testnet-12's `5ee7fd8e…` → `2e567642…` with nothing in force changing). (a) The kernel route's ledger policy carries `ruleset_digest = consensus_params_id`, the route header stores it, and `ensure_route_header` refuses a stored route "folded under another policy": the first node to take ANY later release disqualifies every block with a live claim or demand (measured: `is disqualified from virtual chain (PALW state): a kernel route move is refused: the stored kernel route was folded under another policy`), while un-upgraded nodes accept it — and two builds rolled out across the route's creation create different headers. (b) The G-RULESET envelope (tag 108) must name the same id: an envelope in flight during a rollout registers on old nodes and is dropped by upgraded ones; the next chain block commits the old build's root and the upgraded node disqualifies it (measured: sinks differ, PALW roots differ). The handshake keeps both builds as peers (same identity, same fork id below the new fence) — by design (M1-6). | `g14_c4r3_a_release_that_schedules_an_unrelated_fence_keeps_the_route_folding` (restart over the same DB as the next release); `g14_c4r3_an_envelope_in_flight_must_not_split_builds_that_differ_only_in_a_future_fence` (ignored FAIL) | `consensus/src/pipeline/virtual_processor/processor.rs:1193` (`palw_native_ruleset_id = consensus_params_id()`), used at `:14988` (route extras, before the fix) and `:14906` (envelope); `consensus/core/src/palw_kernel_route_fold_v1.rs:168` | **(a) fixed locally `0fc4adf0f`** (`fix(F-C4R3-01)`: the route's digest is `consensus_identity_id()`, +7 −1 in `processor.rs`; the PoC is green). **(b) OPEN** — the envelope's field is named `consensus_params_id`; proposal: name the ruleset IN FORCE at the block, `fork_id_v1(params, daa).fired` (genesis + fired heights: equal across builds that differ only in future fences, moves at every crossed fence), or the identity plus `valid_until_daa`. Same root cause, local only: `processor.rs:3327/3513` and `consensus/src/consensus/mod.rs:2918` compare the persisted native-settlement snapshot's `ruleset_id` with it (dormant DNS retirement; after an upgrade the EVM canonical heads stop until a resync) |
| F-C4R3-02 | **P1** | **A provable fraud is laundered into a free availability default by keeping ONE valid proof out of the chain for `court_deadline_daa` (20 DAA), not for the window plus the liability horizon (250 DAA).** The producer publishes everything; the colluders' own bond demands any position and the producer stays silent. At the deadline the claim is `Unavailable`: the producer pays `default_penalty` and the rest of the reservation is released; pre-Final the demanders take the WHOLE penalty (Panel-licensed) or 90 % of it (OPV, 10 % burned). The outsider's true proof, filed next, is **dismissed with the filing fee** ("nothing is reserved any more"). Measured (Panel / OPV): censored 21 DAA; producer −100 KAS; colluding demander +100 / +90 KAS; outsider −0.1 KAS; not convicted. Round 1's O-level "slow outsider" (A') on the node: the colluders' cost is 0 KAS (Panel) / 10 KAS (OPV) against a 1,000 KAS slash, and RFC-0015 §6.3's censorship cost (saturate the court through window + liability) overstates the requirement ~12×. | `g14_c4r3_a_self_inflicted_default_must_not_erase_a_provable_fraud`, `g14_c4r3_opv_…` (ignored FAIL) | `misaka/palw/kernel/src/ledger.rs:1789` (pre-Final default paid to demanders), `:1847` (an `Unavailable` claim's reservation released at once), `:1552` (a proof against it dismissed with the fee) | **FIXED `7da2d9ee2` (G14-R4)** — a pre-Final default sets `liability_until = default + liability_daa` and KEEPS the rest of the reservation; a valid proof in that horizon convicts the defaulted claim (state `Convicted`), its bounty the accuser's share of the WHOLE admitted reservation (undiluted, capped by the slash); a default is split like a slash (demanders take `accuser_reward_permille`, OPV at most `1000 − default_burn_permille`, the rest burned); a proof past the horizon or against a timed-out claim is REFUSED (no court, no fee). Withholding with no valid proof stays a default: released at the horizon. PoCs un-ignored; measured after: convicted, outsider fee 0, bounty 500 KAS, colluding demander +50 of the 100 KAS penalty (colluders' net −950 vs −1,000 for a plain conviction) |
| F-C4R3-03 | **P1 where armed** (the chunk lane itself is armed on testnet-12 for `FamilyCertified`; that griefing is ADR-0075 SA-1's named residual) | **Eight junk chunks (1.6 KAS of rent) hold every chunked prosecution off the chain for 4,000 DAA.** A kernel object larger than one carrier — a real class's `FileProof`, a position `Respond`, an onboarding refutation (105) — rides `ObjectChunk`s; the chunk lane is ONE network-wide table of 8 half-assembled groups held until a 4,000-DAA TTL; a chunk is unsigned and anyone may open a group at the flat 0.2 KAS slot rent. Measured: 8 one-part junk groups; the outsider's chunked proof (cut at 1 KiB to stand for a real class's) re-sent six times between DAA 4 and the horizon (254) never opens a group; the lie finalizes at 54 and leaves its liability horizon unconvicted (control: the same chunked proof convicts when the lane is free). The same 1.6 KAS makes an honest producer default on a demanded position whose response needs chunks (and a pre-Final default pays the demander), and keeps a false artifact binding unrefuted until it is Final (window 40, horizon 200). | `g14_c4r3_eight_junk_chunk_groups_must_not_hold_a_chunked_proof_off_the_chain` (ignored FAIL); `g14_c4r3_control_a_chunked_proof_convicts_when_the_chunk_lane_is_free` | `consensus/core/src/palw_state_v2.rs:9103-9105` (8 groups, TTL 4,000), `:35585` (a new group refused when full), `:9304` (kernel / onboarding objects admitted to the same lane) | **FIXED `a09c6119d` (G14-R4)** — the route's OWN chunk lane: `KernelRouteChunkV1` (consensus tag **113**), rows in the route's aux table **41** (42 allocated, unused); a group is keyed `(opener, group)` and every chunk is signed by the opener's Active bond; ≤ 2 open groups per bond; a deposit of 1 KAS per declared part from free collateral (V2's committed ledger and both exit gates read it; forfeited — slashed — at TTL, returned on completion); TTL `min(64 DAA, the target's deadline)` and a group for a target that can no longer use it is refused; the assembled object must be a `FileProof`/`Respond` naming the target claim or a refutation (105) of the target binding. The certification lane is unchanged for every other object. PoC un-ignored: junk fills the legacy table AND the colluders' own rooms; the outsider's proof convicts at the first attempt |
| F-C4R3-04 | P2 (release process, the 9,000 height) | **A fence that joins an already-scheduled height is invisible to the fork id.** The int-13 list's own doc invites the unready G14 lanes to "join by one line here plus the re-pin". Once a fleet runs the int-13 build, a later build adding `palw_probabilistic_constraints_v1`, `palw_panel_free_v1` or `palw_signed_registration_v1` at 9,000 has the same fork id at every height and neither side refuses the other past 9,000, where they disagree about blocks (schedule-id warning only). | `c4r3_observation_a_fence_joining_the_int13_height_leaves_the_fork_id_unchanged` (passes: pins the hazard) | `consensus/core/src/fork_id_v1.rs:298` (heights only); `consensus/core/src/config/params.rs:21880` | OPEN — freeze `PALW_T12_INT13_FENCES_V1` (a pin test on its names) once its build is deployed; a lane that misses it takes a fresh height |
| F-C4R3-05 | P2 (DoS, OPV) | **OPV lane capture.** `max_live_claims_total` is one network-wide counter; a claim counts while its reservation is held — through the window AND the liability horizon (≈ 250 DAA at the interim terms) — and jobs are free. ⌈32 / 3⌉ = 11 bonds fill the lane with HONEST claims on jobs they posted themselves; every other producer's OPV claim is refused while they refill, and the occupiers are paid the (unfunded) `claim_reward` at each Final. Measured with the cap scaled to the harness's eight cards (total 6, per producer 3): two bonds fill it, a third producer's honest claim is refused; an occupier's slot frees at Final (55) + liability (200). | `g14_c4r3_opv_two_bonds_must_not_be_able_to_hold_the_whole_opv_lane` (ignored FAIL) | `misaka/palw/kernel/src/opv.rs:560` (global cap), `:512` (live = reserved > 0, liability phase included); `consensus/core/src/palw_panel_free_v1.rs:75` | **FIXED `f05a672c5` (G14-R4)** — the caps count PRE-FINAL OPV claims only; `max_live_claims_per_producer` bounds each bond, and a producer with no pre-Final claim is always admitted past the total (no set of bonds holds the lane against another; pre-Final ≤ total + #producers, each backing a reservation and a fee); `OpvEconomicsV1::admission_fee` (validated > 0, interim 1 KAS) burned at admission, never refunded (`SettlementKindV1::AdmissionFee`). PoC un-ignored and extended (the honest producer's second claim is refused while the total binds) |

## Observations (no new defect; quantified)

| id | what |
|---|---|
| O-C4R3-R7 | **GAP-R7 quantified on the real path** (`g14_c4r3_observation_gap_r7_a_lifted_proof_takes_the_bounty_and_halves_the_colluders_loss`): a `FileProof`'s proof bytes name no accuser; the proof is lifted from the honest carrier's PUBLIC bytes, re-signed by another bond and included first. The copyist is paid the whole bounty (500 KAS = half the reservation), the outsider's filing is a `Duplicate` (0 KAS, no fee). With the producer's own bond as the copyist the lie costs the colluders HALF its reservation; under any rational block producer an honest verifier's expected bounty is ≈ 0. Also: `OpvPolicyV1::required_reservation` (`opv.rs:150`) does not divide by `1 − accuser_reward_permille/1000`, so the self-recoup halves the margin the relation claims (the interim 1,000 KAS still clears the 120 KAS it computes). **CLOSED `1445dcbc5` (G14-R4)**: kernel route object `SealProof` (kernel tag 15, inside tag 110; seal `H(claim ‖ accuser ‖ H(proof))`); at a conviction the bounty goes to the EARLIEST seal (≥ `claim_seal_delay_daa` old) of the convicting bytes, whoever files them (an unsealed filing counts as sealed in its own block); `LedgerPolicyV1::validate` and `OpvPolicyV1::required_reservation` now price self-recoup (÷ (1 − accuser‰)). The observation test is now the pass/fail `g14_c4r3_gap_r7_a_lifted_proof_pays_its_earliest_sealer_not_the_copyist` (the producer's own bond lifts the sealed proof: sealer 500 KAS, copyist 0, colluders −1,000 KAS). Residual, priced not prevented: the colluders can always self-convict FIRST with a proof of their own and recoup the bounty. |
| O-C4R3-mint | GAP-5 reconfirmed with its rate: `SettlementKindV1::FinalReward` is "newly issued" (`settle.rs:16`) and the node queues it to the coinbase; `PostJob` is free, so a bond answering its own jobs mints `claim_reward` per job — unbounded on the Panel route, ≤ 32 live claims on OPV. Blocking for any activation. **Fixed (G14-R4, GAP-5, the user's ruling: user-pays escrow)**: every posted job reserves `claim_reward` of its poster's bond as an escrow and burns `job_fee`; the job's first Final debits the escrow and pays exactly that — no unfunded `FinalReward` path remains (the fold pays a reward only out of the escrow debit before it); an idle escrow returns after its TTL; a self-posted job is down its fee. See `g14-node-e2e-record.md` §8. |
| O-C4R3-respond | `Respond` is signed by ANY synced bond (no named actor) and a rejected response spends one adjudication of the block's budget with no fee (`ledger.rs:1732`): the budget can be spent free while a demand is open. Bounded in production by the block's carrier count (~125 KB of transactions a block), so not a censorship primitive at 64 adjudications; it does break §6.3's "each junk run forfeits the filing fee" premise. P3. |
| O-C4R3-root | A header commits its SELECTED PARENT's PALW root, so a fold divergence between builds surfaces one chain block later (F-C4R3-01b's PoC mines that block). By design; recorded because a divergence in the sink block is invisible until a child exists. |
| O-C4R3-drop | A kernel object the ledger refuses is "dropped", yet the arm still writes the route header on first use and the block's budget row (`palw_kernel_route_fold_v1.rs` refusal path): deterministic, but the module doc's "nothing is written" is not literal. |
| O-C4R3-v3 | RFC-0010's receipt-clock pause (`palw_panel_v3_fold_v1.rs:310`, the structural guard the Lead asked for) pauses on ANY open DA session or court session and re-bases at the last close — read, holds. Its residual: the 16-session lifetime cap can be spent by the colluders' bonds, but each refuted session costs `r × S_P` (DA-6), so 16 of them cost more than the claim's reward; and every V3 draw ends `BeaconUnavailable` today (`approved_beacons` is empty in every release), so no V3 claim binds. Not exercised on the node by C4r3. |

## Held (negative results)

| attack | evidence | verdict |
|---|---|---|
| **Mandatory test 3, Panel-licensed, node-less**: the producer's signed seal and lying reveal relayed by `misaka_palw_remote::relay::broadcast_signed_tx` (one relay lies and is reported `Tampered`, never counted); every seat signs a passing receipt; an ordinary bonded outsider (read API + public DA, own salt) relays its proof the same way | `g14_c4r3_mandatory_3_a_relayed_lie_covered_by_every_seat_is_convicted_by_an_outsider` | PASS: convicted, the REAL producer bond −1,000 KAS, outsider +500, seats untouched, replay equal |
| **Mandatory test 3, OPV (Panel = 0), node-less**, before and after Final | `g14_c4r3_mandatory_3_opv_relayed_lies_are_convicted_before_and_after_final` | PASS: one convicted in the window, one Final then convicted inside liability; both reservations slashed; replay equal |
| a chunked proof when the chunk lane is free | `g14_c4r3_control_a_chunked_proof_convicts_when_the_chunk_lane_is_free` | PASS |
| F-C4-05 (seed from any beacon) | `VerifiedWorkBeaconV1` has private fields, no decode, built only by the collector/verifier; `challenge_seed_v1` takes it and checks the context id (`misaka/palw/challenge/src/beacon.rs:209`, `seed.rs:30`) | fixed (inspection) |
| arming a G14 fence on a real network | `validate_palw_v2` refuses `palw_probabilistic_constraints_v1`, `palw_panel_free_v1` (value checked first), `palw_signed_registration_v1` at every height; `OverrideParams` has no PALW field (`--override-params-file` cannot arm them; refused on mainnet anyway); no drill flag names them | holds (inspection) |
| G14 objects on testnet-12 below the fence | dropped by name, first and charged nothing, before any slot / rent / counter (`palw_v2_accepted_objects_and_refunds`), matching an older build's undecodable skip (A-2); a chunk group completing into a dormant G14 inner is dropped on both builds (the gate here, `ChunkedObjectUndecodable` there) | holds (inspection) |
| slash clamping / a vanished producer bond | the producer is re-synced before every proof/demand/response and the tick (`kernel reserved ≤ synced ≤ collateral`); `slash_bond` debits `min(amount, collateral)`; V2's exit gates read the kernel and onboarding reservations, so a producer bond with a reservation cannot leave | holds (inspection; lane tests cover the exit) |
| onboarding refutation misjudged | the kernel opening's dtype and shape are inside its commitment (`merkle.rs:248`), rows are contiguous last-axis rows, the byte comparison is two's-complement over `i128` values at the declared width, the V2 leaf is pinned to the layout's index and count; the instance-set proof is exact equality with the program's declared instances | holds (inspection) |
| baseline | the lane's 50 `g14_` tests at `183a761bf`; then all 60 `g14_` tests (lane + C4r3) after `fix(F-C4R3-01)` | 50/50 pass; after the fix 54 pass, 5 ignored (the open FAILs), 1 failed: `g14_registration_replay_on_a_second_node_and_across_a_reorg` — lane D's own GAP 11 (no strict-win fence: the reorg is a tie decided by candidate hash). It failed 3× in a row under machine load and passed 3/3 later on the same source, and passed once with the fix reverted: a flake of that race, not the fix (which is read only where the route's fence is in force; that test never arms it). Worth arming the strict-win fence in that test. **Fixed `39e4ea441` (G14-R4)**: the case runs with strict-win armed (the first reorg is a shallow tie GHOSTDAG decides by blue work) and A's tail carries an attempt, so the deep reorg back is a STRICT economic win — no hash race. |

## Not attacked in round 3

RFC-0010's V3 fold on the node (beyond reading the pause; V3 cannot bind without an approved beacon); pipeline claims on the node (no
Panel, no header wire form); a cross-lane slash race (a V2 conviction draining the producer bond to a colluder before the kernel's,
F-C4-04's clamp then shrinks the honest bounty); the O(rows) per-object fold cost (GAP 8) as a DoS figure; reorg across Final with
sibling blocks in the DAG; beacon grinding (no approved scheme to grind).

## Commits (C4r3)

`654c89ace` PoCs · `d0c1c97d9` F-C4R3-01 (envelope) PoC · `0fc4adf0f` **fix(F-C4R3-01)** · `6af3b894d` F-C4R3-03 PoC + control · this
record.

## G14-R4 fixes (agent G14-R4, branch `g14/r4-fixes`, base `4d3baa0c5`, 2026-10-08)

Everything stays dormant behind `palw_probabilistic_constraints_v1` (OPV: `palw_panel_free_v1`); no t12 identity moves (the new
consensus object is decodable only past the fence, the new rows live in the route's aux table and the kernel's own collections, and
`PALW_T12_INT13_FENCES_V1` is untouched).

| item | fix | commit | regression tests | PoC |
|---|---|---|---|---|
| F-C4R3-02 (P1) | a pre-Final default keeps the rest of the reservation through `default + liability_daa`; a valid proof there convicts (undiluted bounty, no fee); a default is split like a slash; a late / timed-out proof is refused free | `7da2d9ee2` | k2_ledger `a_self_inflicted_default_never_erases_a_provable_fraud_and_a_true_proof_never_pays_a_fee`, k2_opv `an_optimistic_self_inflicted_default_never_erases_a_provable_fraud`; the default E2E cases assert the held reservation and its release | both un-ignored, PASS |
| F-C4R3-03 (P1) | the route's own chunk lane: `KernelRouteChunkV1` tag 113, aux table 41, signed per-bond rooms (2), deposit forfeited at TTL, TTL ≤ the target's deadline | `a09c6119d` | core `the_routes_chunk_lane_assembles_only_coherent_groups_and_bounds_them_by_their_target`; node `g14_kernel_route_the_routes_own_chunk_lane_is_per_bond_deposit_backed_and_bounded_by_its_target` | un-ignored, PASS (1 attempt) |
| F-C4R3-05 (P2) | round 1: only pre-Final OPV claims hold admission slots, per-bond cap, a non-refundable admission fee. **Round 2 (the user: Sybil splitting must not defeat the cap)**: a share of every block's court runs reserved for proofs (`prosecution_reserve_permille`, interim 500‰) and a hard ceiling `total + fresh_producer_slots` bounded by the window's reserved proof runs; holding the whole lane costs one fresh bond + reservation + fee per slot, and continuously `hard × reservation × (W+L)/W` locked + `hard × fee` per window | `f05a672c5`, `70069988c` | k2_opv `live_claim_caps_…`, `n_cheap_sybil_bonds_pay_for_every_slot_and_cannot_hold_the_opv_lane_past_one_window`, `a_flood_of_claims_never_takes_the_runs_reserved_for_proofs` | un-ignored, PASS |
| GAP-R7 | accuser seal (`SealProof`, kernel tag 15 inside tag 110); the earliest seal of the convicting bytes is paid; self-recoup priced | `1445dcbc5` | k2_ledger `a_lifted_proof_pays_its_earliest_sealer_and_self_conviction_still_costs_more_than_the_reward`; OPV relation tests | the observation is now the pass/fail `g14_c4r3_gap_r7_…`, PASS |
| GAP-11 | strict-win armed in the registration reorg case; A's tail strictly heavier on economics | `39e4ea441` | — | the case is deterministic |
| GAP-5 | Final reward paid out of the poster's escrow (user-pays, the user's ruling), once; a non-refundable job fee | see `g14-node-e2e-record.md` §8 | k2_ledger_route `a_final_reward_is_paid_once_out_of_the_posters_escrow_and_nothing_is_ever_issued`; node `g14_kernel_route_the_final_reward_is_paid_once_out_of_the_posters_escrow_across_reorg_replay_and_redemption`; the test consumer refuses an unfunded `FinalReward` in every ledger test | — |

Queued from the lead and landed on the same branch: **OPV-BOOT #2** bonded claim seals and `ClaimRowV1::sealed_daa` (`00b2491cc`);
**K2S's demand-bond fate** (a served position's demand bond is held, refunded on conviction / default / timeout and burned only at an
unconvicted liability horizon; `ca2a2080a`); **OPV-BOOT #1** the chunk lane's `Conformance { v2_class }` target (plumbing; every action
but a Refute rides only a group the class's registrant opened, fail-closed for actions added later; its deadline is OPV-BOOT's
`palw_conformance_chunk_target_v1`, pending, so every such group is refused at its first chunk until it is wired). A seal whose job another
claim took is forfeited at expiry like a withheld one — a refund would make choosing among N claim ids of one job free (accepted by the
lead; the honest race-loss cost `(n − 1)·d` per won claim is sized in `g14-node-e2e-record.md` §8; k2_ledger_route
`a_seal_on_a_job_another_claim_took_is_forfeited_at_its_expiry`).

Round 3 is closed on this branch: **F-C4R3-01(b)** was fixed by OB-P0 on the integration line (the envelope signs the fork-id fired
digest, not `consensus_params_id`) and its PoC passes after the merge of `claude/g14-public-prosecution-integration-9bee39`; **F-C4R3-04**
is moot with no DAA-9,000 flag day.

---

# Round 4 — Agent C4R4 (2026-10-09)

Branch `adv/c4r4`. Attacked (merged into the branch, commits recorded): `g14/r4-fixes` @ `eea63a130` (escrow GAP-5, bonded claim seals,
served demand bonds, accuser seals GAP-R7, reserved proof runs F-C4R3-05 round 2), `opv/bootstrap-beacon` @ `410d430e5` (effective
false-accept accounting, distinct source rules, complete-check policy shape — the challenge-crate half; derived eligibility E1–E7, the
complete-check bootstrap and the sealed-source beacon v3 are not on any branch yet), `fix/partition-rejoin` @ `2d6d77f7c` (N2 watchdog,
N3 readers, `--palw-verified-resync`; F-C4R4-09's PoC and fix were then committed there, at the Lead's request, and merged back), and
the integration line @ `eb75f568e` (PRINCIPLES, registry). Read, not merged: `fix/a2-uniform-new-tags`
@ `37676d2c2` (A2U, WIP), `k2/real-scale` @ `a492011ec` (conflicts with `g14/r4-fixes` in `ledger.rs`/`rows.rs`/`route.rs`/`processor.rs`; an
integration step, not this lane's).

Every target here is dormant (`palw_probabilistic_constraints_v1` / `palw_panel_free_v1` are refused at every real height) or node-local
and not deployed: **nothing in this round affects live int-12**, and no t12 id moves (tests and dormant-fence code only). Convention as in
rounds 1–3: a defect's test asserts the SAFE property and is `#[ignore = "FAIL F-C4R4-nn …"]` until its fix; `obs` tests pin by-design
behaviour, quantified.

## How to run

```text
export CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0
cargo test --offline -p misaka-palw-kernel --test c4r4 -- --include-ignored
cargo test --offline -p misaka-palw-challenge --test c4r4_grinding
cargo test --offline -p kaspa-p2p-flows --lib --test partition_watch_front_runner -- partition_watch front_running first_deliverer genuine_minority
cargo test --offline -p kaspa-consensus --lib g14_c4r4 -- --include-ignored
```

## Findings

| id | sev | what | PoC | root cause | status |
|---|---|---|---|---|---|
| F-C4R4-03 | **P1 where armed** | **A squatter's claim seal froze a poster's escrow — and its whole bond's exit — indefinitely.** GAP-5 returns an idle escrow only while no producer seal of the job is live; a seal is an unbacked promise any bond may make on any unclaimed job, and a re-seal keeps its one deposit and restarts its clock. `Withdraw` needs `reserved == 0` (V2's exit gates read the same row on the node), so one locked seal deposit (1 BILI interim) held the harness poster's whole 1,000,000-unit bond, exit requested, for as long as the squatter re-sealed every < 100 DAA. | kernel `c4r4::f_c4r4_03_a_squatter_seal_never_holds_a_posters_escrow_or_exit_past_the_ttl_and_one_seal_ttl` (+ control: a producer that sealed before the TTL is still paid) | `misaka-palw-kernel/src/ledger.rs` `release_idle_job_escrows` (any live seal blocks the return; re-seal restarts it) | **FIXED `a1473a3d9`**: a seal holds an escrow at most until `posted + job_escrow_ttl + seal_ttl` (a seal placed by the escrow's TTL has one seal TTL to reveal, and expires then anyway) |
| F-C4R4-05 | **P1 where armed** | **One dismissed filing per block holds every valid proof out of the court.** `charge` debits a `FileProof` its class's DECLARED worst court work, before the court runs, from ONE per-block court budget all classes share; registration refuses only a class whose worst court is ABOVE the block's. One junk filing against any proof-open claim of the heaviest class (the attacker's own honest claim serves) spends the block's court work, and every later `FileProof` in the block is `OverBudget` (dropped, free). Cost: ONE `dismissed_proof_fee` per block, not the `max_adjudications × fee` (6.4 BILI a block, ~1,600 BILI over window + liability) of G14-R4's bound and RFC-0015 §6.3's `censorship_cost`. The prosecution reserve reserves runs, not court work, and `censorship_cost` is evaluated per class with that class's own worst court, so a LIGHT class passes it while a heavy class registered beside it (Panel mode: no economics check) makes its prosecution censorable at one fee per block. The court-work cap binds before the run cap for any class whose worst court exceeds `max_court_work_per_block / max_adjudications_per_block` (2^30 / 64 ≈ 1.7·10^7 interim). No crafted class is needed: the test fixture's own class, with its declared long-context history window, has a worst court of 2^27, so 8 filings fill a 2^30 block. Unlike a run flood, which needs ≥ 64 carriers of ~5–10 KB in one block (more than a block holds), this needs one to eight carriers. Measured before the fix (block budget = the class's worst court): 49 blocks censored for 245 in fees (5 a block) against the 320 a block the relation prices; the lie reached Final unconvicted. | kernel `c4r4::f_c4r4_05_one_junk_filing_must_not_buy_a_whole_blocks_court` | `ledger.rs` `charge` / `file_proof` (fee flat, work at the declared worst); `opv.rs` `censorship_cost` (per class) | **FIXED `2dfcee86e`**: a dismissal forfeits `dismissed_proof_fee × ⌈worst_court × max_adjudications / max_court_work_per_block⌉` (at least one fee) — `LedgerPolicyV1::dismissal_fee_v1`; saturating a block's court now costs ≈ `max_adjudications × fee` whichever class the junk names (after the fix the attacker's three scaled fees are 960 = 3 × 320, and the lie is convicted in the fourth block). Unchanged where every class is light (the multiplier is 1: no golden moves). **Owner follow-up (G14-R4):** `censorship_cost` should take the per-block lower bound `fee × max(1, ⌊A·(B − W)/B⌋)` (cross-class), and the end-of-lane item 12 should cite this |
| F-C4R4-10 | P2 (P1 where a block carries ≥ cap − reserve refutations) | **The onboarding court spends the runs reserved for proofs.** A tag-109 `Refute` (any Active bond of another operator; a refutation that proves nothing is dismissed with NO fee) and a `Post` charge the block's budget through `charge_route_budget_v1`, which compares with `max_adjudications_per_block` — the whole block — while the kernel's own `charge` stops every object but a `FileProof` short of `prosecution_reserved_runs()` (G14-R4's round-2 fix). Measured in the four-run harness: four free junk refutations spend all four runs, two of them reserved. At the interim 64-run cap a full bypass needs ≥ 64 refutation carriers in one block, more than a 500,000-mass block of ML-DSA-signed carriers holds; the guarantee the reserve states is nevertheless conditional on that capacity. | node `g14_c4r4_junk_conformance_refutations_must_not_spend_the_runs_reserved_for_proofs` | `consensus/core/src/palw_kernel_route_fold_v1.rs` `charge_route_budget_v1` | **FIXED `63da18025`**: onboarding objects stop at `max − reserved`, as every kernel object but a `FileProof` does; the lane's hostile-evidence test now reads 2 of 4 runs spent (it asserted 4 of 4 before) |
| F-C4R4-11 | P2 (P1 where a block carries ≥ cap refutations) | **Free junk refutations can carry forged conformance evidence through its window.** For the same reason (a dismissed refutation is free), junk refutations ahead of a valid one in every block of the 80-DAA window keep the valid refutation over budget until the forgery passes. The reserve fix does not change it (honest and junk refutations share the runs left). Not reachable at the interim 64-run cap with ~5–10 KB carriers (a block holds fewer refutations than runs); the four-run harness shows the mechanism. | node `g14_c4r4_free_junk_refutations_must_not_carry_forged_evidence_through_its_window` | `consensus/core/src/palw_onboarding_fold_v1.rs` (the dismissed-refutation path charges the budget and nobody) | **OPEN**, routed by the Lead to OPVB (sketch items 1–3; item 4 deferred to a doc follow-up). Measured in the four-run harness: 80 blocks of four junk refutations plus one valid one, and the forged evidence ends `G14Eligible`. Fix sketch: (1) a `Refute` is a proof and may spend the reserve; (2) one judged refutation per (attempt, refuter bond) per window — censoring then needs a fresh Active bond for every run of every block (64 × 80 at interim); (3) a refutation that proves nothing forfeits `dismissed_proof_fee`; (4) optionally, a refutation dropped over budget in the window's last block extends it by one block. A fee alone only prices the attack |
| F-C4R4-09 | **P1 for shipping N2 to any live network** (node-local, not deployed) | **One outbound peer holds a node on its own chain.** The relay flow notes a peer only when it is the FIRST to deliver a block (a known or already-requested inv is skipped before `observe_relay_for_partition`), and "ours" is `is_chain_ancestor_of(sink, block)`, so a DAG sibling (an attempt beside the slot's heartbeat) is noted `Other` even from an honest peer. One attacker-run outbound peer, ten minutes connected, that announces every new chain block first (relaying invs before validating wins every race) collects all `ExtendsOurs` notes; the honest outbound peers are noted only for the siblings they deliver first; given any refusal run the node holds. The module's threat model assumes an eclipse (most long-lived outbound peers); one slot suffices, and an attacker with one outbound slot on many nodes holds them all. The same peer keeps a `--palw-verified-resync` pending forever (it needs two peers noted ours). With no attacker, a systematically fastest honest peer has the same effect. | flows `partition_watch_front_runner` (on `fix/partition-rejoin`; PoC `2180fd2bb`, ignored FAIL there) | `protocol/flows/src/v7/blockrelay/flow.rs` (known/requested inv skipped; `observe_relay_for_partition` after a processed block), `protocol/flows/src/flow_context.rs` `observe_relay_for_partition` (chain-ancestor relation) | **FIXED `d8db65082` on `fix/partition-rejoin`** (the Lead's assignment): `BlockPlace` — ours on this node's selected chain either side of the sink, Other only extending the refused tip's chain, a sibling notes nobody; every announcer is credited (`note_announced` for an in-flight block, bounded 4,096; `observe_relay_for_partition` also for an inv of a known block). The PoC is un-ignored; the old crediting stays pinned in the test (it holds); a genuine minority still holds. The refusal half is not described here |
| F-C4R4-01 | P2 (design; dormant) | **The distinct source rules tell bonds apart, not parties.** Two Sybil producer bonds and two Sybil poster bonds, one work on each of two eligible classes, settled first, own EVERY position of a `Distinct` and a `DistinctClassCapped` beacon; honest Finals settled later are never read, and since every input is the attacker's, all of them are ground offline (4,096 nonce pairs → 4,096 beacons, an 8-bit target hit). The rule prices capture (k producer bonds, k jobs, k OPV reservations; a tiny complete-check class makes the second class cheap); it does not stop it. | challenge `c4r4_grinding::f_c4r4_01_…` | `misaka-palw-challenge/src/beacon.rs` `canonical_attributed_sources_v1` (first-k by settlement) | OPEN (OPV-BOOT, SG-01a): mix every eligible Final of the window, or v3 with a seal window that closes before any reveal; account the distinct rules as a cost |
| F-C4R4-08 | P2 (cross-lane accounting) | **The grinding bound reads a live cap that no longer bounds the Finals of a beacon window, and no state names a Final job's poster.** OPV-BOOT's `beacon_grinding_choices_bound_v1(k, live_cap, F)` takes `live_cap = max_live_claims_total` because "every work that can settle in one window is live at once"; since F-C4R3-05 only pre-Final claims hold slots, a slot frees at Final and refills. Measured (example policy: total 5, fresh 2, window 50; a 120-slot beacon window): 14 OPV Finals, `P(14,2) = 182` lists, not `P(5,2) = 20`; at the interim 32 + 16 / 50 / 120: 96 Finals, 14 bits not 10. And the only state naming a job's poster is its escrow row, removed when its Final is paid (`KernelJobV1` has no poster): a node deriving `consumer_id` from state sees `Absent` for every Final source, so "distinct consumers where known" never binds. | kernel `c4r4::f_c4r4_08_…` | `misaka-palw-challenge/src/soundness.rs` (live-cap premise), `misaka-palw-kernel/src/opv.rs` `opv_admission` (pre-Final caps), `ledger.rs` `pay_from_job_escrow` | OPEN (OPV-BOOT): `C = hard × ⌈(beacon_window − window)/window + 1⌉`; record the poster on the claim or job row (a ledger table change, G14-R4) |
| C4R4-A2-113 | pin (known in-flight A-2 member) | Tag 113 (`KernelRouteChunkV1`, G14-R4) runs its may-ride arm at isolation: an unsigned / out-of-range chunk is `ObjectMayNotRide` where int-12 tolerates undecodable bytes (A2U's member #3 shape). A2U §3.4 names it; its exhaustive owner match forces the row at merge. | node lib `g14_c4r4_a2_a_tag_113_carrier_below_its_fence_is_judged_as_undecodable_bytes` (ignored FAIL until A2U and G14-R4 meet) | `consensus/core/src/palw_lifecycle_objects_v2.rs` (113's may-ride arm) | in A2U's queue |

## Observations (quantified; no new defect)

| id | what |
|---|---|
| O-C4R4-02 | **Watcher absence** (`c4r4::f_c4r4_02_obs_…`): with no honest verifier through window + liability, a provable OPV lie is paid the escrow (7), keeps its whole reservation (1,000) and is permanent; producer profit = reward − admission fee (4), consumer loss = reward + job fee (9), nothing slashed. The honest-verifier assumption is load-bearing (PRINCIPLES §5), not enforced. |
| O-C4R4-06 | **The served-demand-bond rule sets the verifier's cost** (`f_c4r4_06_obs_…`): nothing obliges a producer to publish; against one that serves only on demand, checking an honest claim costs the verifier one burned demand bond per position (5 positions, 50 of the test bond), the producer nothing. Combined with O-C4R4-02, PRINCIPLES §6 condition 6 (verifier incentive) is not met by the bounty alone. |
| O-C4R4-07 | **The producer's Sybil takes the bounty of its own conviction** (`f_c4r4_07_obs_…`): it seals its own proof at once; a verifier's `SealProof` names the claim in clear a seal-delay before the filing, so the Sybil files first: verifier 0, Sybil 500, colluders −500 of 1,000 (deterrence holds as G14-R4 priced it). Measured: the proof bytes do NOT depend on the verifier's salt (the producer's and the outsider's proofs are byte-equal), so the pre-seal captures the bounty passively, with no race at all. Against a rational liar a verifier's expected bounty is zero: a verifier incentive must come from something the accused cannot capture (or blind seals that do not name the claim). |
| O-C4R4-12 | **The losing sealer of a job race forfeits its deposit** (`f_c4r4_12_obs_…`): once another claim holds the job, a sealer can never reveal, and its seal expires as if it had withheld (1 BILI). v3 should not read such a forfeit as a withheld source; a deposit whose reveal another claim made impossible could be returned when the job is taken. |
| O-C4R4-capture | **F-C4R3-05 round 2's capture price is an upper bound** (`f_c4r4_obs_capture_…`: a slot held 50 DAA plainly, 78 with a self-demand served late). The end-of-lane formula holds each slot for one window; a colluder's own demand filed at the window's last DAA and served at the deadline's last stretches a slot's pre-Final life to `window + deadline + grace` (`OpvPolicyV1::hard_deadline`, 80 DAA interim), cutting the locked collateral of a continuous capture from `48 × 1,000 × 250/50 = 240,000` to `48 × 1,000 × 280/80 = 168,000` BILI (−30 %) for one burned demand bond per slot-cycle. |
| O-C4R4-respond | **O-C4R3-respond after the reserve.** A rejected `Respond` (any bond) still spends a run for free; the reserve halves the runs it competes for (32). Forcing an honest producer's default this way (a valid response kept `OverBudget` until the demand's deadline, the demander then taking its share of the penalty) needs ≥ 32 junk carriers per block — near a block's capacity of ML-DSA-signed carriers. G14-R4's end-of-lane item 9 (producer-only `Respond`, or a fee) closes it. |
| O-C4R4-Q | **The adaptive term `Q` is stated, not enforced.** Effective bits subtract `⌈log2 Q⌉` for the subjects an adversary may commit; the chain keys conformance attempts by `(class, artifact)` (`apply_conformance_committed_v1`), so a new V2 class over the same artifact gets a fresh `retry_limit + 1`. Nothing caps attempts per artifact (or per program), so `Q` is the adversary's budget over the registration price. E6 needs a per-artifact attempt counter (or the tuple must state `Q` against that price). E6 must also read the approved tuple's reviewed relation soundness, never a scope's declared fault density (round 1's GAP-C4-D). |
| O-C4R4-A2-2 | **A-2 two levels down, after the full activation.** A2U's inner table owns kernel-route inner kinds by TAG; a new variant nested inside an existing inner kind (K2S's `ProsecutionV1::Segmented = 3` inside `FileProof`, R4X's `ProsecutionV1` 4) is owned by neither that table nor the int-12 manifest (int-12 decodes no kernel type). Both ship in the one full-activation release, so no two builds differ on them today; the first release AFTER it needs a frozen kernel wire-type manifest (the int-12 manifest's mechanism) so a later nested variant rides unjudged below its fence. |

## Held

| attack | evidence | verdict |
|---|---|---|
| pre-Final-default liability horizon edge (F-C4R3-02) | `c4r4::f_c4r4_04_…`: a proof at `default + liability_daa` convicts (900 held, 500 bounty); one DAA later refused free | holds |
| escrow double pay / mint | read: `pay_from_job_escrow` removes the row; a later Final of the same job pays 0; no `FinalReward` outside it (G14-R4's tests) | holds |
| demand-bond fate ordering at the horizon | read: a conviction at `liability_until` refunds (inclusive); the tick burns only at `liability_until + 1`, after the same block's objects | holds |
| A2U's inner-kind gate vs a build without the kind | read: both drop in the acceptance walk's gate (`palw_kernel_route_object_is_signed`), before the fold writes a header or budget row | holds |

## Results (milestone 1 on `adv/c4r4`, one slot)

* Kernel PoCs on the UNFIXED ledger (PoC commit `e57e81bb9`): F-C4R4-03 and F-C4R4-05 FAIL as asserted; every observation passes with the
  numbers above.
* With the fixes: kernel `c4r4` 9/9; `k2_ledger` / `k2_ledger_route` / `k2_opv` filtered to escrow, seal, dismissal and self-inflicted
  tests: pass; challenge `c4r4_grinding` 1/1; flows `partition_watch` unit tests 5/5 and `partition_watch_front_runner` 4/4 (the N2 fix on
  `fix/partition-rejoin`); the node `g14_` suite 71 pass, including F-C4R4-10's PoC and OB-P0's conformance suite.
* Still failing, as asserted: F-C4R4-11 and the tag-113 pin (`--ignored`).
* One `g14_` failure that is not this round's: `g14_kernel_route_a_chunked_object_is_signature_checked_at_the_completing_chunk` asserts
  "reserves nothing" after a tampered chunked claim, but the unrevealed claim's seal deposit (1 BILI, G14-R4's bonded seals `00b2491cc`)
  is reserved until its TTL — a stale expectation, routed to G14-R4 by the Lead.

## Commits (C4R4)

`e57e81bb9` PoCs · `a1473a3d9` fix(F-C4R4-03) · `2dfcee86e` fix(F-C4R4-05) · `63da18025` fix(F-C4R4-10) · on `fix/partition-rejoin`:
`2180fd2bb` PoC and `d8db65082` fix(F-C4R4-09), merged back, and into the integration line at `b5678e4ff` · this record.

## Not attacked in round 4

OPV-BOOT's derived eligibility E1–E7, the complete-check bootstrap (`PostComplete`) and the sealed-source beacon v3 — not on any branch
at `410d430e5` (design reviewed: F-C4R4-01/08, O-C4R4-Q; `PostComplete` will charge `charge_route_budget_v1`, which now respects the
reserve). N3's sink readers (RPC-only). K2S's real-scale route (`k2/real-scale` @ `a492011ec` does not merge cleanly with `g14/r4-fixes`).

---

# Round 4b — Agent C4R4, successor (2026-10-10)

Branch `adv/c4r4` after merging `g14/r4-fixes` @ `0ef5084d1`, `opv/bootstrap-beacon` @ `6e9dfa065` (its fix of the merge's 3-argument
DA16 call made the branch compile again) and, through both, the integration head `b8ae9412b` (the user's `pre`: ADR-0175/0176/0177,
ADR-0032 49%), then INTF's `int/rs49-fence` @ `f7ed9642c` (the 49% behind `palw_reporter_share_v2`). The round-4 fixes `a1473a3d9`, `2dfcee86e`, `63da18025` are
unchanged. Priority (user, 2026-10-10): **G14 under full collusion** — the producer and every Panel seat collude; one public bonded
verifier outside the Panel must reach a conviction, the correct DA default, or the dismissal of an honest claim, from public
authenticated material only. Then ADR-0032's 49% as self-dealing, then ADR-0177 non-interference.

Every target is dormant (the kernel route, OPV, the provider court: validation refuses arming) or a pure function: **nothing here
affects live int-12**, and no t12 id is moved by this round (tests only). `pre` had made R-core's share 49% unfenced (moving the t12
ids); INTF's merged fence restores the historical 10% below `palw_reporter_share_v2`, and the 49% tests below use its `*_v2` functions.
Cross-references to G14C's matrix (`g14/completion` @ `82a176a77`): F-C4R4-19 is its GAP-05 with a PoC, F-C4R4-17 its GAP-06 with a
PoC; F-C4R4-13 is the kernel analogue of the t12 note that a bystander's accusation is outrun by the timeouts (C8).

## How to run (round 4b)

```text
cargo test --offline -p misaka-palw-kernel --test c4r4 -- --include-ignored
cargo test --offline -p kaspa-consensus-core --test c4r4_reporter_share_49 -- --include-ignored
cargo test --offline -p kaspa-consensus --lib -- --include-ignored g14_c4r4
cargo test --offline -p kaspa-p2p-flows --lib --test partition_watch_front_runner -- partition_watch front_running first_deliverer genuine_minority
```

## N2 verification (F-C4R4-09)

Merged tree, one batched run with `--no-fail-fast`: flows `partition_watch` 5/5, `partition_watch_front_runner` 4/4 — F-C4R4-09's PoC
(a front-running outbound peer neither holds the node on its own chain nor keeps a `--palw-verified-resync` pending) passes, the
first-deliverer crediting of `2d6d77f7c` stays pinned (it held), a genuine minority still holds. **Verified** (node-local; not armable:
no live deployment decision).

## G14 under full collusion — the five questions

| # | question | result |
|---|---|---|
| 1 | pre-empt / censor the outsider's filing | **Held** at the kernel: a self-conviction still convicts (the bounty is captured, O-C4R4-07); filings and refutations that take a run pay a fee (F-C4R4-05, OPVB's F-C4R4-11 fix), so censoring a block costs about `max_adjudications × fee` for the whole window + liability; the chunk lane is per opener (F-C4R3-03). |
| 2 | force a timeout, or withhold so the case ends in no default | **Broken on the Panel route: F-C4R4-13.** Withholding ends in a default (held, F-C4R3-02); a demand holds a `Checking` claim; but a service one DAA past the receipts' deadline times the claim out in the serving block. |
| 3 | reach Final before the outsider can file | **Held**: proof grace after a service, post-Final liability with the reservation held, the horizon edge inclusive (F-C4R4-04). |
| 4 | make the outsider need secret state | **Conditional (ADR-0177)**: a kernel fault proof opens a weight column; the chain never serves weights, so without the model p = 0 (O-C4R4-Q4). The only on-chain route to the weights is the cumulative scope ADR-0177 forbids (F-C4R4-17). |
| 5 | exhaust the shared court budget | **Broken for the honest-claim leg: F-C4R4-14** — a rejected `Respond` is the last free run consumer. Proof runs are priced (row 1). |

## Findings (round 4b)

| id | sev | what | PoC | status / owner |
|---|---|---|---|---|
| F-C4R4-13 | P2 where armed | **A Panel-route claim served past its receipts' deadline times out in the serving block**; the proof the service enables is refused ("ended without passing"). The colluding Panel never covers; the producer serves the TRUE values one DAA past the deadline: no conviction, no default, collateral 5,000 → 5,000. A fraud caught in `Checking` is free. `ProofGrace` holds Final, not the timeout. | kernel `c4r4::f_c4r4_13_…` (ignored FAIL) | OPEN → G14R. Fix sketch: `Checking` times out only at `daa ≥ final_hold_until` (lifecycle, two arms: `Tick` and the late `Tally`) |
| F-C4R4-14 | **P1 where armed** | **Free rejected responses force an honest producer's default.** A rejected `Respond` pays nothing but spends a non-reserved run before classification. 32 junk a block × 20 DAA keeps the valid response `OverBudget`; at the deadline the honest producer pays the default (100), the griefer-demander takes 50, the claim is voided. (b) works with the junk signed by the griefer AS THE PRODUCER of its own self-demanded claim, so a producer-only `Respond` does not fix it. Needs ordering (a mining griefer, or fee priority) over 20 DAA. | kernel `c4r4::f_c4r4_14_…` (ignored FAIL; (a) and (b) both: 640 free junk over 20 DAA, default 100, griefer paid 50) | OPEN → G14R (its end-of-lane item 9). Fix sketch: a rejected response pays `dismissed_proof_fee` (checked free before the charge), a serving response may spend the proof reserve, and `validate()` requires `(A − r) × court_deadline × fee > accuser_share × default_penalty` |
| F-C4R4-15 | P2 where armed (ADR-0032) | **A default then a self-conviction recycles more than 49% of what they collected.** F-C4R3-02 pays the bounty "as if no default came first", but the default's demander share was already paid from the same reservation. At 490‰: collected 1,000 (default 100 + slash 900), colluders recovered 514 (24 demander share + 490 bounty) = 51.4%; net loss 48.6% < 51%; up to 53.9% with many Sybil demanders. No profit (the claim earns nothing). | kernel `c4r4::f_c4r4_15_…` (ignored FAIL) + control (plain self-conviction: exactly `⌊49%⌋`, holds) | OPEN → G14R. Fix: hold the pre-Final default's demander share to the horizon and burn it on a later conviction (or cap share + bounty at `s × collected`) |
| F-C4R4-16 | P2 (dormant) | The kernel route's interim `accuser_reward_permille` is **500**, above ADR-0032's 49%: a self-reporter nets −50%. | core `c4r4_reporter_share_49::the_kernel_routes_interim_…` (ignored FAIL) | OPEN → G14R / INTF (one constant + the kernel tests' bounty figures) |
| F-C4R4-17 (= GAP-06) | P2 design conflict (dormant) | **Claim-specific demands rebuild the registered model.** 20 position demands of ONE claim (200 BILI of demand bonds) made an honest producer serve what rebuilds 11 of 13 MatMul weight instances exactly (2,256 of 9,506 weight elements), each opening the registered commitment. ADR-0177 D2 forbids the cumulative scope and asks existing units to be restricted in the new ruleset; G14 needs the faulty position demandable. | kernel `c4r4::f_c4r4_17_…` (ignored FAIL) | OPEN → G14C (matrix) + G14R + POLICY: either a closed model is p = 0 (and earns as such) or a demand scope that cannot reach rank |
| F-C4R4-18 | P3 (exactness) | R-5's `X = min(lock, ⌊G_res/k⌋)` floors the DEDUCTION, so a post-Final self-conviction can net up to `0.49 × (k−1)` sompi above ADR-0032's `−51% × (ΣS − G_res)` (G_res 2, k 3, lock 1, action 99: −151 vs −151.98). Per conviction R-1 holds exactly (below). | core `…::r_core_the_post_final_…` (ignored FAIL) | OPEN → INTF (with the 49% fence): `div_ceil` |
| F-C4R4-19 (= GAP-05) | P2 (dormant; ADR-0177 D1/D2) | **The provider court as it stands interferes with model acquisition**: an artifact binding needs two availability leases; a challenge naming a weight leaf is accepted; leaving it unanswered slashes the lease (150 KAS each); with both charged the pair lapses and the binding attests nothing. | node `da16::c4r4::g14_c4r4_adr0177_…` (ignored FAIL, 4 violations listed) | OPEN → DA16's ADR-0177 re-scope |

## Results (round 4b, on the merged tree with INTF's fence)

* Kernel `c4r4` with `--include-ignored`: 11 pass (round-4 tests, the 49% control, O-C4R4-Q4), 4 FAIL exactly as asserted
  (F-C4R4-13, -14 (a)+(b), -15, -17).
* `kaspa-consensus-core --test c4r4_reporter_share_49`: R-1 at 49% and the DA-6 observation pass; F-C4R4-16 and -18 FAIL as asserted.
* Node `g14_c4r4` with `--include-ignored`: **F-C4R4-11's PoC passes** (OPVB's `b5d4ba90c`: a refutation is a proof, judged once per
  bond per window, pays the fee — verified here) and F-C4R4-10's restatement passes; the tag-113 pin (A2U) and F-C4R4-19 FAIL as asserted.
* Node `g14_` + `partition_rejoin` + `live_r1` (before the INTF merge): 97 pass, 1 FAIL that is G14R's (below).

## ADR-0032 49% — held

* **R-1 per conviction (verified):** `R = ⌊4,900 × max(0, collected − X)/10,000⌋ ≤ ⌊49% × collected⌋`, so a self-reporter's loss is
  ≥ `⌈51% × collected⌉` over 198 `(collected, X)` pairs, rounding, `X > collected` and `u64::MAX` included. One offence pays one reward
  and a DA default rewards only the producer's debit (signers' S4 burned), so funding one's own reporter Sybil recycles ≤ 49% per slash.
* **DA-6 at 49% (observation):** a refuted session now burns `⌈49% × S_P⌉` — 1,568.49 MSK at the floor's `Live` base, ×4.9 the 10%
  figure; with ADR-0177 (nothing published proactively) that is the honest verifier's price of asking for material, so the 49% share
  does not improve PRINCIPLES §6 condition 6. The doc's "the refuted cost never exceeds the reward" holds only up to rounding
  (cost 1,568.49 vs reward 1,568.4655 MSK on the ADR's example).

## ADR-0177 non-interference

* Kernel route: a demand is a committed POSITION (node values, stage inputs) — no weight unit, no weight is ever served; a court reads
  the registered commitments; a peer that refuses to serve a model triggers nothing. **But** the union of position demands is a
  cumulative scope (F-C4R4-17).
* Provider court (DA16, not yet re-scoped): F-C4R4-19.
* Armed t12 courts: DA units are claim material (event tiles, held state/prompt/step ranges, step leaves/nodes, row nodes) — no weight
  unit; a third party's refusal to serve a model is no input. The cumulative-scope concern applies in principle to step leaves, bounded
  on t12 by DA-6's price and DA-8's caps (16 non-seat sessions per claim, three drawn units each); ADR-0177 asks the NEW ruleset to fix it.

## Next round

ADR-0176's bypasses — bond split, exit/re-register, key change or collateral transfer resetting the `d + W` clock or the consumption
history (ADR-0176 lines on "bond分割・再登録・鍵変更・担保移転") — once lane BUDGET's engine (`palw_bond_budget_v1`) exists; nothing to
attack yet (`budget/adr176-177` = the integration head).

## Not this round's (routed)

* G14R's `g14_opv_the_salted_seal_rows_roll_back_and_reapply_identically_across_a_reorg` fails on the merged tree (the reorg does not
  happen; internal) and `k2_opv::past_the_fence_a_withheld_seal_is_kept_as_forfeited_…` expects two beacon seals where its own S1 fix
  now keeps three (stale expectation).
* The tag-113 A-2 pin stays A2U's (still an ignored FAIL here).
