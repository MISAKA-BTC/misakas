# Lane GC-DUTY: Gate C drawn verifier duty for 'S'/'V' rows and attestor liability for 'S'/'K'

Branch `claude/gatec-duty` (base `claude/g14-panel1-17000` f82d8db94). Not pushed. The repo uses no cryptographic proof of computation (PRINCIPLES §8). Certification here is the drawn verifiers' signed approval, given under the honest-fraction premise.

## 1. Node duty for 'S' and 'V' rows (always on, armed by identity)

- Consensus read: `palw_certification_duties_v1(state, params, me, daa)` in `consensus/core/src/palw_security_certification_v1.rs`. It returns the 'S'/'V' rows (aux 136) that are drawn and undecided, have `daa < deadline`, list `me` among the draws, and hold no attestation from `me`. Rows come back earliest deadline first. A chain claim's row includes lane B's job descriptor. The read returns nothing below Gate C (`gate_c_at`). It is exposed through the API as `ConsensusApi::palw_certification_duties_v1` (virtual processor impl).
- Node: `kaspad/src/palw_cert_duty.rs`, called once per tick from the `palw_panel` loop after the audit duty. Steps:
  - It re-executes the claim on the audit duty's replay seam (`resolve_backend`, `attempt_job_for_claim`, `reserve_replay_at_widest_run_v1`, `execute_for_verdict`, `palw_operator_da_verdict_v1`).
  - It signs inner 75 `AttestEligibilityExamV1` with `exam` set to the certification id, on court round `u32::MAX-44`.
  - A replay that reproduces gives `Reproduced`, with a transcript digest that is never zero. A refuted replay gives `NotReproducible`. A row the node cannot judge gives `Unavailable`. Unjudged replays retry every 30 DAA and attest `Unavailable` once the row is within 60 DAA of its deadline. Since veto = 0, `Unavailable` cannot block certification.
- Resources: the duty has its **own single replay slot** (its own `PalwSeatReplaysV1`, 1 slot), so full seat slots cannot starve it, which was the failure in the 10-26 incident. Memory is still bounded by the shared host ledger reservation (`--palw-host-memory-share`).
- **Known coverage gaps.** These rows get only `Unavailable`, so they will **not certify**:
  - 'V' (owning39 mining): kaspad has no replay for native mining claims.
  - 'S' rows that lane B does not replay (`palw_operator_da_replayable_v1`): free-prompt (FP) families, `held_to_final`, and claims with no artifact root. **REAL Attempt claims are covered; LegacyFusedLeaf and FP are not.** A replay seam for these is needed before the fence.

## 2. Attestor liability for 'S' and 'K' (new dormant fence `palw_gate_c_attestor_liability_v1`)

- New `Params` field, `None` on every preset. Mirror field `attestor_liability_from_daa`. Validation requires Gate C to be armed at or above it. It goes into the identity normalisation, the fork-id hashes and the fork-id probe as Some-only, so a None value moves nothing.
- A positive attestation past the fence locks the attestor's free collateral, using the G-N2 relation and key (aux 143 `0x03`) over the claim's gain:
  - 'S': `max(reserved, held_reward)`
  - 'K': the held amount
- An attestor that cannot back the lock is not counted. Negative or withheld attestations lock nothing.
- 'S' conviction: `reverse_convicted_final` charges the locks (slash). Row retirement releases them.
- 'K': the sweep applies G-K1's fate rule (`palw_kernel_signer_lock_fate_v1`): Charge, Release or Hold.
- I added the 0x03 129-byte arm to the native wire scanner and its generator. Without it, 0x03 rows (which owning39 already writes) hit "unallocated". The generator idempotence check has not been run.

## 3. `security_certification: "not-governed"`

The real RPC path (`palw_producer_v2.rs` claim rows) already calls `palw_claim_certification_status_v1`. The remaining hard-coded values are test fixtures only (`palw_panel.rs` own_claim_event_tests, `palw_producer_backpressure.rs` tests, `palw_producer_v2.rs` tests). No change was made.

## 4. Expiry branch for uncertified 'S'/'V' claims

- 'S': the held issuance leg is never named (burned). There is no poster to refund, because an Attempt escrow comes from subsidy. User escrow settles under the existing Final rules.
- 'V': the native fold closes the claim with no receipt and no credit (`palw_native_mining_fold_v39`). Holds and attestor locks are released.
- No change was needed.

## 5. Commit hook for PanelDa claims

Not done.

## Tests

- New tests, which pass:
  - `gc_duty_a_conviction_slashes_only_the_positive_attestors_and_a_retirement_releases`
  - `gc_duty_the_node_read_names_drawn_unattested_open_rows_only_past_gate_c`
  - three kaspad unit tests in `palw_cert_duty`
- Failures seen on this branch in consensus-core `rfc18_*`, `gatec_decouple_*`, `a2u::*` (tag 245) and `r5_f02_wire_*`. These assert Gate C `None` on presets, while the base already has 16600, or they concern code this lane did not touch. I believe they come from the base branch, but I have not checked them against the base.
- Not done: the real-node E2E (≥8 bonds → REAL → Final → 41 attestations → SecurityCertified → payout; failure modes; reorg/restart). It is left to lane GC-E2E, which should drive this duty.

## Round 2 (2026-10-27, user: 17,400 only after replay for the three types)

Seams (kaspad `palw_cert_duty`, own slot, host ledger; core read now carries the claim as a `PalwSeatDutyV2`):
- (a) LegacyFusedLeaf / held-to-Final Attempt: lane B's attempt replay (`execute_for_verdict`), now admitted for `held_to_final` in the heavy slot.
- (b) FpSelection/FpConstraint/FpPrefix/FpTokenizer (+ held): SEAT-R's `fp_seat_replay_pass_v1` over the pooled/retained job material.
  Without the material on this host the row stays Unjudged and becomes Unavailable at the 60 DAA margin (material availability, not a replay gap).
- (c) 'V' owning39: **Unavailable kept.** kaspad has no native39 executor (params/material/trace archive); `palw_native_mining_v39` is not in the
  17,400 list, so no 'V' row can exist on the release. Wiring the SDK's `execute_independent_replay_v39` into kaspad is open.

Tests: route per type + honest→Reproduced / lie→Refuted on each seam's verdict rule (unit, not a real backend replay); consensus E2E
`gatec_duty_e2e_drawn_bonds_find_their_duty_and_the_honest_claim_certifies` (Attempt only; the replay is stood in by root equality).
NOT run: a real-node replay per type; FP/fused/'V' E2E.

Fence: `palw_gate_c_attestor_liability_v1` in `PALW_T12_G14_17000_FENCES_V1` right after Gate C (same height), in ACT2's slot (ACT2 0c63ddf72 merged).

Base-stale fixes: ACT2 merge fixes `rfc18_*`/`gatec_decouple_*`; fork-id probe arm for `palw_selected_source_bootstrap_v1` added.
Still failing (base, need owners): fork-id probe next missing `palw_source_verifier_economics_v2`; a2u tag 245 and 255 have no allocated
A-2 row ("ask the Lead"); `r5_f02_wire_*` (court supply hold, post-final default no longer dormant); kaspad
`seat_r_tests::past_seat_r_only_the_replays_can_end_a_verdict_with_valid` (source scan expects `execute_free_prompt`); native wire
generator fails its handwritten-codec shape review for `PalwStateCarriageV2` (idempotence check could not complete).
