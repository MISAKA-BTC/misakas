# G14 lane r6-pause-deps — 2026-10-22

Status: **written, not compiled, not run** (lane rule: no cargo). Branch `g14p/r6-pause-deps`, base 3c6e4c09d.
Fences: no new fence. Reuses RFC-0018 Gate C (`palw_independently_work_certified_v1`, via the RFC-0018 mirror) for item 1,
`palw_court_clock_v2` for item 2, and `palw_court_clock_v2` + X3 `palw_claim_hard_deadline_v1` for item 3.
**Arm centrally:** Gate C (needs `PALW_RFC0018_ARMABLE_V1 = true`, user decision RFC-0018 §12), `palw_court_clock_v2`, X3.

## 1. Receipt pause — rooted row (user decision 2026-10-22)

Storage choice: **kernel route aux table 145**, key `b'P' ‖ claim` (65 B), row `PalwReceiptPauseRowV2 {version u8, claim [u8;64],
open u8, open_since_daa u64, paused_total_daa u64}`. Journaled by the existing `KernelRouteRow` delta, rooted in the aux root, carried
by the existing tail `0xEC` — **no new carriage tail byte** (the 75 `*_TAIL_V*` bytes untouched).
Why not aux 144: table 144 already holds claim-local `F`/`R`/`C` AND RFC08's native held fee `0x01`; `assert_claim_local_context_v1`
treats every non-`C`/`R` key in 144 as a liability frame and refuses it, and the generated scanner has two `144=>Err` arms (the first,
claim-local's, shadows the native-hold arms). A new prefix in 144 would be refused at load. **Pre-existing collision found (not this
lane's):** RFC08 08-B native held fees (`0x01` in 144) collide with the claim-local namespace exactly that way — flag for central.

Code:
- `consensus/core/src/palw_receipt_pause_v2.rs` (new, `pub mod` in lib.rs): table/prefix/row, `palw_receipt_pause_row_v2` (malformed row
  -> typed `PalwReceiptPauseMalformedV2`, never "no pause"), `palw_receipt_pause_account_v2`, `palw_receipt_pause_claims_v2`.
- `consensus/core/src/palw_receipt_pause_fold_v2.rs` (child of `palw_state_v2`, `reconcile`): ONE writer at a fixed place — end of
  block, after `palw_staged_court_fold_v1::sweep` (all objects, sweeps, court closes done). Follows THE predicate
  `palw_accusation_pending_v1` instead of hooking each accusation path: governed+certified+Final claim newly pending -> open at this
  block's DAA; open row no longer pending -> `dismiss(now)` (span credited); claim voided (conviction) or gone -> row deleted.
  Candidates come from the indexes the predicate reads (open courts, DA sessions, legacy dispute holds, court-service bindings,
  staged TIR rows) — no walk of all claims. No-op unless `gate_c_at(daa)` and the kernel route exists.
- `palw_rfc0018_v1::palw_fp_receipt_clock_v2`: not governed -> `palw_fp_receipt_clock_v1` byte for byte; governed -> reads the row and
  applies `palw_fp_receipt_clock_paused_v2` (r5).
- Callers switched v1 -> v2 (all production readers): `palw_fp_admission_v3.rs:197` (V3 and RFC-0009 V4 admission),
  `palw_state_v2.rs` ReceiptSpend fold (~19718), `consensus/src/.../processor/palw.rs` spendable RPC (~3677) and own/merged pre-check
  (~10136, coinbase entitlement), `kaspad/src/palw_service_tir_frame_e2e.rs:898,1490`. (Tests keep v1 where they test v1.)
- Native wire: `scripts/drills/gen_rfc06_native_wire.py` (aux arm 145) and `gen_rfc06_native_inventory.py` (row type) updated;
  **generated files not edited** — central regen must emit the 145 arm (`wire_kernel_aux.rs`), `wire_struct!`/`native_struct!` for
  `PalwReceiptPauseRowV2`. Until then `aux_registry_tests` will fail on table 145 (expected).
- Test helper `PalwChainStateV2::set_claim_for_tests` (`chain_state.rs`, cfg(test)).

Semantics note: the window "start" moves by the paused DAA, so draw slot / use window both shift; the fold's ReceiptSpend arm only
checks `is_err` of the clock (it does not itself enforce the window from the returned value) — unchanged, as before.

## 2. Dependency horizons (F07)

`palw_exec_v2_rewards_v1.rs::court_clock_v2_exec_provider_dependencies_v2(state, params, claim_id, now)` reads the rooted rows:
- **Exec**: if `claim_id` is an EXEC root — the root's signed floor (`exec_liability_floor_v1(expiry, window_court, kernel policy)`),
  and EACH kernel claim it rests on (prefix claim + every slice evidence root) at its own `liability_until`, separately (per-dependency,
  as F07); missing kernel row / missing horizon -> typed `CarriageInconsistent` refusal. A retired root's unpaid receipt
  (`exec_v2_rewards`) -> its `liability_until`.
- **Provider**: for those kernel claims and `claim_id` itself as a `ProviderSubjectV1::KernelClaim`, when DA was transferred (tag 153):
  horizon = `palw_court_clock_v2::palw_provider_quorum_horizon_of_v2` = the 2nd-longest (PALW_PROVIDER_MIN_PROVIDERS_V1) distinct
  non-producer operator's live lease end; lapsed / < quorum -> 0.
- EXEC origin rows (`exec_v2_origins`: source_block, issued_daa) carry **no horizon** — nothing to check there.
- Wired in `palw_legacy_public_filer_fold_v1.rs::apply_dispute_reserved_v1` beside the r5 custody dependency, only when a NEW record
  is created on a clock-v2-governed claim; refusal happens before any write (atomic). Below the fence nothing is read.

Design decision / open question (user): a dependency the producer (or its providers) controls can now make the extension — and so the
reservation — refused. r5 already had this shape for custody. Whether a short dependency should instead (a) be refused at the claim's
ADMISSION (so it never arises) or (b) let the dispute proceed and charge the short dependency, is a policy choice; this lane only
mechanises the brief's "refuse atomically". Same r5 consequence applies: with `network_v1` windows a governed staged claim's dispute
extension may be refused until windows cover `a+10,000+max(W_disclose, reorg)`.

## 3. Rename-safety (X3 × clock v2)

New `palw_court_clock_v2::palw_claim_x3_cap_hard_v2`: X3 not governing -> `None` (old rule); X3 governing -> `a+10,000` when clock v2
also governs, else X3's hard. Switched every site that CAPS by X3's hard:
- `palw_state_v2.rs` licensed Final floor cap (REQ-X3-2, ~4134);
- `palw_econ_contracts_fold_v1.rs` EXEC root native claim's cap in the horizon extension (~652) (+ doc in `palw_econ_contracts_v1.rs`);
- `palw_bond_budget_fold_v1/consumer.rs` paid consumer material must cover the hard (~56).
Already combined (r5): pursuit start (`palw_claim_pursuit_may_start_v1` delegates), TIR install (`palw_claim_route_hard_v2`), dispute
record hard (`palw_dispute_record_hard_deadline_v2`), staged fold (`palw_claim_court_hard_deadline_v2`). Not switched (deliberate):
`palw_settlement_liability_fold_v1` native KERNEL claim rows (`palw_x3_hard_deadline_at_v1(policy, row.accepted_daa)`) — those are not
V2 claims, clock v2 does not govern them; `palw_claim_retention_obligation_v1` (already max'd with v2 retention in write_claim, r5).

## Tests (crate `kaspa-consensus-core`, lib)
- `palw_fp_admission_v3::tests::r6_receipt_pause_row_opens_on_accusation_and_dismissal_resumes_the_remaining_window`
- `palw_fp_admission_v3::tests::r6_receipt_pause_row_follows_a_reorg` (revert_delta_v2 of the opening and of the closing block)
- `palw_fp_admission_v3::tests::r6_receipt_pause_row_is_dropped_on_conviction_and_a_malformed_row_refuses`
- `palw_court_clock_v2::tests::r6_provider_quorum_horizon_is_the_quorum_th_longest_operator`
- `palw_court_clock_v2::tests::r6_exec_and_provider_dependencies_refuse_a_short_extension`
- `palw_court_clock_v2::tests::r6_x3_cap_reads_the_combined_rule`
Run: `cargo test -p kaspa-consensus-core --lib r6_`.

## Risks / uncertain
- Fold tests drive the pause with `set_open_courts_for_tests` (an index, not a real court session); if a debug derived-index check
  rebuilds `open_courts_by_claim` from `court_sessions` the fixtures need a real session. The conviction test sets the claim `Voided`
  directly (`set_claim_for_tests`); a DL-1 debug deadline-consistency check could object.
- `revert_delta_v2` state-root equality in the reorg test assumes derived indexes are not rooted.
- No fold-level test for item 2 (needs an EXEC root + kernel claim rows + provider leases fixture) — pure tests only; central E2E.
- Reconcile cost: per block past Gate C, walks the pause rows plus the candidate indexes (bounded by their own caps); `da_claims` may
  hold closed records (filtered by `open_sessions() > 0`, still iterated).
- Pre-existing aux-144 namespace collision (above).
