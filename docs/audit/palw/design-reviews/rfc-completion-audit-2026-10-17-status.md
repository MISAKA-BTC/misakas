# RFC completion audit 2026-10-17 — status of the six assigned items (integ/validation-r2)

Checked against the code at `ca5e75080` (the audit read older code). All six are still open in code. Each needs values or a
design choice that ADR-0181 reserves to the user; those are listed. No fence is armed; t12 identity unchanged.

| item | still present | what a real fix needs | user decision needed |
| --- | --- | --- | --- |
| RFC08 unsourced liability | `refuse_unsourced_admission_v1` at the four native admissions (`palw_bond_budget_fold_v1.rs` funded v11, text, job v39, mining v39); `Unsourced` legs for the material provider fee (`palw_monetary_material_fold_v1.rs`), drawn-verifier fees and beacon-source fees (`palw_kernel_route_fold_v1.rs`), which `settle_once_liable_v1` closes with a zero terminal marker | a producer-signed absolute liability horizon on each native job/claim record (today only `deadline_daa` / `commit_deadline`, not a liability horizon), a monetary-covenant TTL bound to the claim (the kernel-claim path already sources both: paid lifetime + `covenant_ttl_v1`), the bond hold until the horizon, and explicit hold → release / forfeit / refund transitions per leg | the horizon formula per native type (relative to deadline? court + grace?), whether the fee legs are held to the payer's horizon or the provider's, and the refund rule on default |
| RFC07 PanelLicensed event | `final_export_v1` returns `None` for `PanelLicensed` (the route does not hold the Panel's seed/epoch); staged trap reveal refused for interactive claims (`palw_paid_claim_trap_v1.rs` "a trap on an interactive (staged) paid claim is not supported") | carry the Panel assignment (seed, epoch, seats, dependency profile) into the route at licence time and export an event marked Panel-dependent (`validity_independent = false`), so the sealed collectors keep refusing it as a source | whether a Panel-licensed event may count anywhere (it cannot be a Panel-independent source by rule) — if not, the export is for audit only |
| RFC09 remote miner L2 | `misaka/palw/remote/src/miner.rs` attested mode still reads `PalwForkChoiceOpeningV1` / `verify_fork_choice_v1` and STOPs under Rule E | port the RemoteNode fetch, opening types, attestation, candidate window and comparator to the V2/V3 root reader the full node uses; 2-fork test under Rule E + Gate C | none (implementation) |
| RFC10 end-to-end with approved sources | no node test drives discovery → auto binding → replacement → Final without Panel injection | depends on the paid-source path (§6 of the validation record), itself blocked by the onboarding-window / paid-lifetime choice | the window/lifetime choice (pending) |
| RFC12 tooling | `scripts/drills/misaka-rfc12-acceptance.py` `NODE_GAPS`: reserve balance read, crash failpoints at import WAL barriers, paid-request generator from an authenticated snapshot, outpoint UTXO read; signed deposit-refund CLI | four node reads/hooks and a CLI; then the acceptance run | none for the reads; the run needs the `evm` build (disk) |
| RFC18 payout split | `name_certified_reward_v1` (`palw_security_certification_fold_v1.rs`) releases the held reward as one leg | split into legs below — conservation proof per option | the ratios (not chosen here) |

## RFC18: design options for the certified reward split (no ratio chosen)

Let `R` be the held issuance reward of a certified claim (already drawn at Final; no new draw). Every option must satisfy
**conservation**: the legs sum to exactly `R` (rounding remainder to a named leg, never minted), each leg is named once through
`settle_once_liable_v1`, and a conviction before release withdraws all unpaid legs (burned, not redistributed).

1. **Two legs, fixed share** — producer `R·(1−p)`, certifying verifiers (Gate C drawn set) `R·p` split equally; remainder to the
   producer. Simple; `p` is the only value.
2. **Three legs with buyback** — producer, verifiers, and a buyback/burn leg `R·b` (removed from supply, not paid). Conservation:
   `paid + burned = R`; issuance net of the burn is lower than the schedule by `b`.
3. **Vested producer leg** — option 1 or 2 with the producer leg released over `V` DAA (linear), unvested remainder forfeited on a
   later conviction inside the liability horizon. Conservation holds per tick; a forfeited remainder is burned.

Values to decide: `p` (Panel/verifier share), `b` (buyback), `V` (vesting), and whether the verifier leg is per drawn seat or per
signing seat.

## Progress 2026-10-18 (integ/validation-r2)

User decisions recorded since: ADR-0185 (fixed-seed beacon bootstrap, seal-commitment lock randomness, paid sources, onboarding
window 1,024 DAA) and ADR-0184 (RFC08 native liability horizon, RFC07 record-only event, RFC18 single certified leg). RFC07's
record-only event is implemented (`KernelFinalReadV1::panel_record`); RFC18 needs no change.

**Item 1 — conformance / opv_bootstrap migration to the real paid path:** see the validation record §8
(`consolidated-validation-2026-10-15.md`). Two consensus bugs past the dormant monetary fence fixed (PAV3 origin header cap, native
covenant lookup); 37 / 54 in the last full group run, more passing in targeted runs since. Two causes need a user decision:
v4/v5 opt-in (and the Panel v5 beacon) cannot reach their `final_by` with approved paid sources by construction, and the
onboarding binding's 200-DAA liability ends before a paid-source conformance can pass.

**Item 2 — RFC08 (ADR-0184), not implemented yet. Implementation plan** (read-only design pass, file:function targets):

- New dormant fence `palw_native_liability_horizon_v1`; only claims committed at or past it take the new rule.
- Horizon derived, not a new wire field: `final_floor + window_court`, where the Final floor is already a deterministic function
  of the producer-signed commit body and the ruleset at commit (funded v11: commit + `ExecutionHorizonsV11.challenge_window`;
  text v29: `NativeTextClaimV29::challenge_end`; job v39: `NativeAcceptedClaimV39::final_after`; mining v39: the Master Final DAA).
  The producer's signature covers every input of the formula — no wire or object-kind change, so nothing new has to ride
  unjudged below the fence. (If ADR-0184's "署名" must be a literal signed number, a versioned field on each of the four wires is
  the alternative.)
- Admission: replace the four `refuse_unsourced_admission_v1` calls (`palw_bond_budget_fold_v1.rs` funded v11, text v29, job v39,
  mining v39) with `admit_claim_liability_v1(.., NativeClaim { family })` past the fence, hard deadline = acceptance + the policy's
  worst-case Final-floor offset + `window_court`; material TTL `u64::MAX` (these families carry no covenant).
- A sourced leg `PalwLegLiabilityV1::NativeClaim { claim, family, .. }` (terms as the `KernelClaim` branch, horizon from the
  derivation); at Final the payee's fee is held (no `write_payout`), released by a sweep past the horizon through
  `settle_once_liable_v1`; a conviction before it refunds the requester and slashes (`slash_bond`); a default refunds the held fee to
  the requester; the native mining receipt vests at the horizon.
- Out of scope of ADR-0184 (stay `Unsourced`): the material provider fee (event-relative covenant), a drawn verifier fee outside a
  paid V2 claim (RFC07-5), the EXEC kernel fee without a reward row, the beacon source fee, mesh audit pay.
- 08-C (EXEC without `exec_v2_test_admit_class_v1`): a fixture with normal class registration, family certification, seat readiness
  (the native-Final suite's `ready_native_seats` already proves inventory readiness) and bond share; the native paid-material test
  fails today for exactly this (`Candidate under the model registry`).

**Item 3 — RFC09, not started. Map:** op 203 (`GetPalwForkChoiceOpening`) serves V1 leaves only; the shared V2/V3 reader is
`palw_verify_committed_leaf_v2` (`palw_fork_choice_rule_e_leaf_v3.rs`), with window/registry sub-openings in the v2/v3 leaf modules;
`misaka/palw/remote/src/l2.rs` stops at the Rule E fence (`LeafV1Insufficient`, l2.rs ~940) and already holds an unused
`verify_committed_opening_v2`. Needed: a V2/V3 opening read (op versioning), the remote comparator over those openings, and the
2-fork Rule E + Gate C test (none exists; `l2/tests.rs` has the V1 conflict-stop tests).

**Item 4 — RFC10, not started** (depends on the paid-source path above).

**Item 5 — RFC12, not started. Map:** the four `NODE_GAPS` (`scripts/drills/misaka-rfc12-acceptance.py` 69–78) have no node read
today (reserve balance store has no RPC; no failpoints; no paid-request generator; no by-outpoint UTXO read). A new read follows
the op registry pattern (`rpc/core/src/api/ops.rs`, message types, service handler, gRPC and wRPC routes). No deposit-refund CLI
exists (`misaka/apps/cli` has `evm deposit-lock` / `claim`); the refund spend needs the lock outpoint (by-outpoint read), the
timeout rule (`EvmDepositLockNotRefundableYet`) and an ML-DSA P2PKH signature script as in `validator-core` funding.
