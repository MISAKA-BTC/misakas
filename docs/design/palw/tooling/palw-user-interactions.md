# PALW user interactions — tooling survey (CLI / RPC / SDK)

> **Scope.** Tooling only: how a user or operator reads PALW state, builds and signs objects, and follows outcomes. **No consensus
> rule, fence, wire tag, preset or parameter is changed here.** The read-only views are built from the same functions the fold and
> the older reads use. Values consensus holds typed-missing are reported as "not adopted"; builders that depend on them refuse.
> Base: `b1eb33960` (integration of the G14/RFC lanes), branch `tools/palw-cli`.

User guide: [docs/guides/palw-cli.md](../../../guides/palw-cli.md).

## 1. Existing surfaces (before this lane)

| Surface | Where | What it covers |
|---|---|---|
| wRPC/gRPC ops 167–255 | `rpc/core/src/api/ops.rs` | Per-subsystem reads, for example: `getPalwClaims` (177: a bond's claim rows with phase, deadline, `securityCertification`, `claimLocalSettlement`, locks), `getPalwSettlement` (182), `getPalwNodeStatus` (179: boot fences as heights, no names), `getPalwLegacyDispute` / `getPalwLegacyDisputes` / `getPalwFraudFilerStatus` (204–206), `getPalwKernelClaim` (210), `getPalwPanelV3Status` (220), `getPalwForkChoiceOpening` (203), `getPalwPaidClaim` (247), `getPalwAcceptanceReads` (246), `getPalwRfc0016Report` (255) |
| `misaka` (crate `misaka-cli`) | `misaka/apps/cli` | The user CLI. Relevant commands: `bond register-* / status / capability / retire`, `palw claim / settlement / vesting / submit-object / da-accuse / shard-accuse / court-close / tir-registration / gen-registration / fp-submit / panel …`, `model …` |
| `palw-class`, `palw-native-*` | `misaka/palw/sdk` | Registration and audit tools for model authors and auditors |
| `palw-remote-miner`, `palw-fork-choice-issuer` | `misaka/palw/remote` | Remote mining, and checking state proofs |
| `misaka-palw-fp-rail`, gateway | `misaka/palw/gateway` | Free-prompt commitments |
| Drills / probes | `scripts/drills/*.py`, `tools/` | Measurement harnesses; these are not user tools |

## 2. Interactions, their tools and the gaps

Status vocabulary:

- **R** — a read exists.
- **B** — a builder exists.
- **—** — nothing exists.
- **NA** — consensus holds a needed value typed-missing (not adopted), so no builder can produce an object the chain accepts.

| # | Interaction | Consensus wire / function | Before this lane | Gap → this lane |
|---|---|---|---|---|
| 1 | Bond status, collateral, exposure, locks | `PalwBondStateV2`, `reserved_exposure`, `palw_bond_live_locks_v1` | R: `getPalwProducerFacts` (167), `getPalwClaims` (177, locks), `misaka bond status` | No single bond document. **Covered:** op 228 `getPalwBondDetail`, `misaka palw bond-detail` |
| 1b | Bond capital assignment (tag 140) | `palw_capital_assignment_message_v1` | — | No builder and no budget read (ops 250–253 are designed but not built). Left open |
| 2 | Claim lifecycle: phase and the date the phase ends by itself | `palw_claim_phase_deadline_v1`, `palw_claim_rows_bind_by_anchor_slot_v1` | R: op 177 (by bond only) | No read by claim id. **Covered:** op 225 `getPalwClaimDetail` |
| 2b | Certification (Final vs SecurityCertified, Gate C) | `palw_claim_certification_status_v1`, `palw_gate_c_governs_v1` | R: op 177 field; the CLI never printed it | **Covered:** op 225 (`securityCertification`, `gateCGoverns`) |
| 2c | Accusation pending, receipt pause | `palw_accusation_pending_v1`, `palw_receipt_pause_row_v2` (aux 145) | — | **Covered:** op 225 (`accusationPending`, `receiptPause`) |
| 2d | Claim-local settlement | `palw_claim_local_settlement_v1` | R: op 177 field | **Covered:** op 225 |
| 3 | Court: open (tag 7), close (8/19/20) | `validate_court_opened_v2`, `adjudicate_court_close_v*` | B: `palw court-close`. R: op 170 | No court state read. **Covered:** op 226 `getPalwCourtState` (ladder turn, per-turn deadline, backstop). The `CourtOpened` builder is left open |
| 3b | DA accuse / disclose (17/18) | `palw_da_accusation_message_v2` | B: `palw da-accuse` | — |
| 3c | Shared court ops (tag 178) | `palw_court_shared_op_carriage_v1` | — | Needs `PALW_COURT_SERVICE_PROFILE_V1` (NA) for supply and defaults. Reported by `palw not-adopted` |
| 3d | Position terminal / lookahead (201–204) | `palw_tir_position_terminal_v2` | — | NA: witness budget and frontier bound. `palw build position-close` refuses |
| 3e | Material intake parts (tag 255) | `material_intake_schedule_v1` | — | NA. `palw build material-intake` refuses |
| 3f | D2 readiness root / demand / answer (230–232) | `PALW_D2_READINESS_DRAW_CAP_V1`, `PALW_D2_DISCLOSURE_FEE_SCHEDULE_V1` | — | NA. `palw build d2-demand` refuses |
| 3g | Post-Final duty accusation / answer (kernel actions 47/48) | `palw_native_post_final_duty_v1` | — | Its default slash is typed-missing (`PALW_NATIVE_POST_FINAL_DEFAULT_SLASH_V1`, in a crate-private module). Left open |
| 3h | Dispute reservation / release (154/155) | `palw_dispute_reserved_object_v1`, `palw_dispute_released_object_v1`, `palw_legacy_dispute_reservation_check_v1` | R: ops 204–206. B: only the node's own filer | **Covered:** `palw build dispute-reserve` and `palw build dispute-release` (signed with the consensus builder; refused while `palw_legacy_public_filer_v1` is dormant) |
| 4 | Funded original: TFO1 envelope with payer consent, escrow rows, TER1 relay, payer return | `PalwTirFundedOriginalEnvelopeV1`, `FundingTermsV1`, `palw_escrow_*` | — | NA: `palw_funded_original_schedule_v1` (profile ids, fee, capital) and `palw_escrow_fee_schedule_v1`. `palw build funded-original` refuses. The escrow-row read is left open |
| 5 | Deadlines and clocks: notice 3,120, hard 10,000, per-turn | `palw_court_clock_v2` (`palw_claim_court_hard_deadline_v2`, `…_normal_hard`, `…_extended_hard`), `turn_deadline_daa`, ladder `last_deadline_daa` | — | **Covered:** op 225/226 `courtClocks`, plus each session's `turnDeadlineDaa` |
| 6 | Court capacity, outsider lane | `palw_court_capacity_fold_v1`, `court_outsider_lane_active_v1` | — | Op 225 shows the per-reserver capacity row (debug form) and `courtReserveUsed`. The ingress fee is NA. The capacity policy is a placeholder that real networks refuse |
| 7 | Model registration, container binding, semantics, tiers | `ClassRegistered*`, tag 175, tag 164, `palw_model_tier_at_v1` | B: `palw tir-registration`, `gen-registration`. R: ops 192–198, 230, 245 | No builder for container binding or semantics, and no tier read. Left open |
| 8 | Fence / activation status | `Params::palw_fences_v1`, `PALW_T12_FULL_ACTIVATION_FENCES_V1` | R: op 179 (heights only) | **Covered:** op 227 `getPalwFenceSchedule`, `misaka palw fences`, `misaka palw status` |

## 3. What this lane added

### RPC

Read-only, node-local, served as versioned JSON in `viewJson`. Ops 225–228 are the unused tail of the 220–229 range; gRPC fields
follow the `1234 + 2·(op − 210)` rule, giving 1264–1271.

| Op | Method | Document (`kind`) | Built by |
|---|---|---|---|
| 225 | `getPalwClaimDetail {claimHex}` | `palwClaimDetail` | `palw_user_views_v1::palw_claim_detail_json_v1`, which uses `palw_claim_row_v1` (the same row builder as op 177) |
| 226 | `getPalwCourtState {claimHex \| sessionHex}` | `palwCourtState` | `palw_court_state_json_v1` |
| 227 | `getPalwFenceSchedule {}` | `palwFenceSchedule` | `palw_fence_schedule_json_v1(params, virtual DAA)` |
| 228 | `getPalwBondDetail {bond}` | `palwBondDetail` | `palw_bond_detail_json_v1`, which uses `palw_bond_live_locks_v1` and `palw_claim_rows_v1` |

- **Clock.** Every state view is judged at the next block's DAA, with the raw second-clock depth and `palw_objective_offence` there.
  These are the facts `getPalwClaims` passes.
- **Answers.** A malformed id is an error before any state is read. `available: false` means the node has no ConsensusV2 state.
  `found: false` means the chain does not hold the named object.
- **Feature detection.** A node built before these ops drops the WebSocket on them, because wRPC has no reply for an unknown op.
  The CLI turns that into exit 36 and names the older read.

### CLI (`misaka palw …`)

**Reads.** Each prints a human view by default, or JSON with `--json`:

- `claim-detail`
- `court-state`
- `bond-detail`
- `fences [--armed-only] [--list L]`
- `status [--bond B]`

**Offline:**

- `not-adopted` lists the typed-missing register from this build's consensus code.
- `object-show --object F [--at-daa D]` reports the tag, variant, owning fence, `may_ride` and `kind_in_force_at`.

**Builders:**

- `build dispute-reserve` and `build dispute-release` write a signed object file.
- `build funded-original`, `court-ingress`, `material-intake`, `d2-demand` and `position-close` refuse with exit 10 and name the
  typed-missing value.

**Submission.** `submit-object` now accepts `--submit` as an alias of `--yes`. Without it, the command is a dry-run.

**Typed-missing register.** `palw_user_views_v1::palw_typed_missing_values_v1` reads each value through the accessor the fold reads.
A row turns "adopted" by itself the day a reviewed change adopts the value. Until then a consensus-core test fails, so the guide and
this survey are revisited.

## 4. Gaps left open

- Builders: `CourtOpened` (tag 7), `BondCapitalAssignedV1` (tag 140), container binding (175), class semantics (164),
  post-Final duty actions (kernel 47/48), shared court requests (178).
  - Each has consensus support, but needs either an interactive step (court) or a typed-missing value.
- A stateful preflight RPC for a dispute reservation (`palw_legacy_dispute_reservation_check_v1` is reachable through the consensus
  API, but no RPC exposes it). Today the builder checks the key and the fence. The fold's own check runs at the carrying block.
- Funded-original escrow rows and TER1 relay template reads. The schedules are NA.
- Model tier read (`palw_model_tier_at_v1`, a borsh-skipped mirror).
- Bond-budget reads (ops 250–253, designed in `economics/bond-budget-and-model-allocation.md` §8.4).
- No SDK (TypeScript/WASM) bindings for ops 225–228. Only wRPC Borsh, gRPC and Rust are covered.
