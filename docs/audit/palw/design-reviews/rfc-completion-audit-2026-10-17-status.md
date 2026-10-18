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

## Progress 2026-10-18, second pass (`8be23fcdf`)

User decisions of 10-18 implemented behind dormant fences (ADR-0185 / ADR-0184 notes): v4/v5 and Panel v5 beacons read opted-in
sources by seal-reveal (`collect_opt_in_seal_reveal_beacon_v6`, node / Panel history / SDK read the same rule); the onboarding binding
liability is derived (1,208 DAA); RFC08 08-A — the four native admissions are sourced with the derived horizon bound (signed
deadline + `window_court`), `refuse_unsourced_admission_v1` is gone. Verified: `opv_bootstrap` 20/20 (was 13/20, including v4, v5 ×2,
eligibility-lost), pruned-import, `rfc0010_production_fold` 26/26, `rfc0010_beacon_adapter` 9/9, the challenge crate.

Still open: x8 assertion rewrites (≈ 10, see validation record §8); RFC08 08-B (hold the native fee at Final to the horizon,
release / refund / slash — needs each family's terminal state machine to stay convictable after Final) and 08-C (the no-seam EXEC
fixture); RFC07 `panel_record` has no dedicated test yet (every Final read in the suites passes with it `None`); RFC09, RFC10, RFC12
not started.

## Progress 2026-10-18, third pass (`integ/validation-r2`)

**Item 1 (this lane, done as far as listed):** x8 rewrites and their results — validation record §9. RFC07 `panel_record` has a
dedicated test. Open in item 1: withheld-slice (an unattributed 5 KAS slash of the root card), native happy-Final (covenant lookup at
the anchor-slot attempt), and a re-run of the six rewritten tests.

**Items 2–5 move to the Codex lane.** State handed over:

- **RFC08 08-B (partial, committed `2c608053d`):** dormant `palw_native_liability_horizon_v1` (params / identity / fork-id / state
  mirror borsh-skipped; fingerprints unchanged); `consensus/core/src/palw_native_liability_hold_v1.rs` (aux 144 row) and
  `palw_native_liability_hold_fold_v1.rs` (hold / release sweep / refund); native job v39 only: `palw_native_job_fold_v39.rs`
  `credit` holds the fee, `credit_after_final_conviction`, `accusable_v39`, `accusation_horizon_v39`, `native_dispute_open_v39`;
  `NativeJobBookV39::convict_after_final` (`palw_native_job_v39.rs`); court / DA request / DA default accept a held Final
  (`palw_native_protected_fold_v39.rs`, `palw_native_public_fold_v39.rs`); Master waits for held components
  (`palw_native_master_fold_v39.rs`); sweep hooked in `palw_kernel_route_fold_v1.rs` closing tick. Core unit tests only. Remaining:
  text v29 (`palw_native_text_fold_v29.rs` `settle` ~1268) and funded v11 holds and post-Final accusation; mining receipt vesting;
  node E2E. ADR-0184 note added.
- **RFC08 08-C (fixture written, failing):** `Cw::no_seam`, `admit_class_without_seam` (exec_slices.rs), two tests in
  `exec_slices/native_final.rs`. The class never leaves `Candidate` in 12,000 DAA: ADR-0147's network-drawn admission jury
  (`transition_builder.rs` `admission_jury_v1` ~6487; audit period, seed anchor of the span before), then Prefetching → Probation.
  Observation: this jury is a model-level review in the legacy registry path, which PRINCIPLES §9 says is not adopted — flagged, not
  changed.
- **RFC09 (implemented, test not passing yet, `aad79f5e5`):** op 203 v2 (committed leaf, v3 window, v3 registry openings; rpc core /
  gRPC / service; `ConsensusApi::palw_fork_choice_openings_v2`); `misaka/palw/remote/src/l2_rule_e.rs` (verify with
  `palw_verify_committed_leaf_v2`, `verify_window_suffix_v3`, `palw_rule_e_verify_registry_v3`, `palw_rule_e_pair_from_openings_v3`,
  `palw_rule_e_order_v2`; STOP on missing/tampered); miner `rule_e_mode_v2` replaces the `LeafV1Insufficient` STOP; node test
  `hb_fork_choice_probe/finality_consistency/rule_e_remote_v2.rs` fails at setup (Gate C needs the kernel route fence — now armed,
  not re-run). Issuer still signs v1 attestations only.
- **RFC10:** not started (needs the paid-source Panel v5 beacon wired into `t12_permissionless_panel_e2e.rs` instead of
  `reference_source()`).
- **RFC12 (partial, `24af8f6aa`):** op 246 `getPalwAcceptanceReads` (reserve balance per block, UTXO by outpoint) + harness
  `acceptance_reads`; pruning-import crash points compiled into the private-integration build; `misaka evm refund` with
  `build_deposit_lock_refund_tx`. Remaining: paid-work request generator, reserve reconciliation rule, harness arming of the crash
  points, the `evm` build and the acceptance run (not run).

## Pre-merge Codex measurements (commit c514deee4)

The measurements below apply to c514deee4 before adopting validation 8478ec5dc. Shared implementation conflicts now use validation; these are historical results, not merged-tip acceptance.


## Codex handover 3 — 2026-10-18 (in progress)

User authorized base `042e5e759` and branch `codex/rfc08-09-10-12`, with RFC09/RFC12 first. RFC08 08-B/08-C and RFC10 remain
waiting for a newer `integ/validation-r2` commit to merge. Validation-lane changes win any conflict; the touched paths must be
identified in the merge/fix commit message. No preset, activation height, identity or frozen design is changed.

**RFC09 implementation in progress:** op 203 builds the committed leaf using the full node's V1/V2/V3 grammar and exposes bounded,
versioned window/registry sub-openings. Empty proof requests retain request-v1 bytes; below the commitment fence the RPC retains
its legacy diagnostic leaf separately from authenticated openings. The issuer checks openings through the common reader before
signing. The remote Rule E path uses that reader, selected-parent walks, authenticated window suffixes and fork-registry
membership/absence/capital, the shared Rule E accumulator/decision functions, and the finality/provenance guards. Missing or
invalid evidence refuses the Rule E path before signing.

**Verification:** tests added, not yet reported as passing: core grammar/root/point tampering and sub-opening completeness;
RPC v1 compatibility / v2 round trips; remote issuer grammar/version binding; actual-node op-203 V2/V3 reads; a two-branch node
comparison under Rule E + Gate C (the no-certified-credit case), including missing and tampered evidence. Initial targeted core
build is in progress. No RFC completion claim is made. Socket-level RPC, populated certified-work fork behavior and the miner's
signing refusal still require verification; RFC12's four capabilities, signed refund CLI and acceptance run are not implemented
by this change.

**RFC12 partial implementation (same handover, unverified):** added the non-default `rfc12-import-failpoints` build feature,
which reuses the environment-and-owned-directory armed OS-kill rendezvous previously available only in unit tests. Existing
UTXO clear/copy and import-completion checkpoints are included, together with before/after synchronous barriers for session
begin and PALW/EVM/legacy sidecars. The private profile manifest advertises whether the feature is compiled. Default node builds
retain no pause/environment path. Acceptance-driver wiring and the crash/restart run remain outstanding; this does not close
checkbox 3 or the other three node-tool gaps.

**Verified core subset (2026-10-18):** `palw_fork_choice_rule_e_leaf_v3::tests` **5/5 pass**, including the two new RFC09 tests
and the three existing V3 root/certification/compatibility tests. Command-local core test optimization is disabled; the first
execution at the default 2 MiB test-thread stack aborted with stack overflow. The unchanged test binary passes with
`RUST_MIN_STACK=67108864` (the validation lane's documented debug-stack setting). RPC/remote/node tests remain unverified at
this checkpoint; no end-to-end completion is claimed.


### Codex handover 3 continuation — 2026-10-18 (implementation, acceptance pending)

- RFC09: core leaf/opening tests 5/5 and RPC V1/V2 transport tests 2/2 passed. A check of RPC service, gRPC core, remote
  binaries (`rpc` feature) and CLI passed before the subsequent RFC12 additions; current-tip verification is still required.
  The real-node tests have not run yet: their first compile found a missing `VirtualStateStoreReader` trait import in the new
  RFC12 read, now corrected. No socket-level or populated certified-work fork result is claimed.
- RFC12: added read-only op 214 (`getRfc12State`) for block reserve balances and up to 64 exact UTXO points, retaining a single
  virtual-generation lock for all requested points and propagating store errors. A missing reserve row is a known zero only
  with retained validation data/genesis, or is read from the matching pruning snapshot; unavailable history stays unknown.
  Added `evm deposit-refund` and an owner/timeout-bound ML-DSA builder; script-engine and tamper tests are written, not yet
  reported passing. The request generator runs inside the exact private paid-v4 factory, checks node genesis/params identity
  and authenticates the complete carriage against an explicitly pinned live header before creating a V4 request. The V4
  request codec is shared with the exporter without changing its bytes.
- RFC12 acceptance driver: exact WAL rendezvous is armed after genesis initialization and before the owned import link;
  restart clears arming. Bridge checks now read original inputs and materialized outputs by outpoint, link the refund to its
  original input/body and check value conservation, and compare those rights after restart/reorg/join. Python controls:
  existing acceptance tests 52/52, new point-read controls 4/4, exact-WAL protocol controls 4/4 pass (synthetic unit controls,
  not real-process observations). Full acceptance, reserve accrual/drip reconciliation and all runtime tests remain open.
- All changes remain dormant with respect to shipped consensus presets. No RFC08/RFC10 implementation has started; the fetched
  validation branch still names `042e5e759`, so the required newer validation commit has not yet been merged.

### 2026-10-18 — RFC09/RFC12 follow-up verification

- Actual consensus harness `t12_fork_choice_commitment`: **4/4 pass** (V1 fence compatibility, V2/V3 op203
  openings/subproofs, two real no-credit forks under Rule E + Gate C + capital, exact UTXO and known-zero reserve reads).
  The two-fork fixture now copies each full node's served DNS gate facts into the issuer's attestation. Missing subproofs
  and substituted committed roots refuse. This is an API harness, not socket-RPC or populated certified-credit coverage.
- `rfc12_refund_is_timeout_bound_owned_and_validates_with_the_real_script_engine`: **1/1 pass** through the real script engine,
  with changed output/amount/timeout, wrong key, early refund and excessive-fee refusal controls. No private-node
  broadcast/reorg/restart refund run has been recorded yet.
- Private paid-v4 node with EVM and `rfc12-import-failpoints`: compile check passed. The generator additionally shares the
  exporter's producer-input allocation preflight and receives the acceptance driver's pinned `{REQUEST_BLOCK}`. Current-tip
  CLI/gRPC checks, standalone input preflight tests, V3 miner signing-refusal controls and positive reserve/drip API assertions
  are still being verified; compilation alone is not runtime acceptance.
- Python exact-WAL controls **5/5 pass**; an exact rendezvous cannot share its launch with a coarse import-phase kill.
  The earlier 52 acceptance controls and four point-read controls remain unit controls only. Full acceptance and independent
  reserve-flow reconciliation remain open. Documentation checks report zero new issues (43 existing baseline occurrences).
- A fresh fetch still names validation base `042e5e759`; RFC08 08-B/08-C and RFC10 remain gated on the required newer merge.

- Follow-up results: current private node + CLI + gRPC client compile check passed. The actual-chain
  `pos_v2_reserve_drip_pays_finalized_epoch` test passes **1/1**, checking op214's consensus read against the
  independently calculated positive slash split, its later coinbase drip, and the unchanged historical balance.
  The retirement acceptance phase now records bounded per-block reserve reads; independent process-level flow
  reconstruction remains open. Retirement reconciliation controls pass **13/13**; acceptance controls remain **52/52**.

- Remote results: `l2::tests` **17 pass, 1 ignored** (the ignored item is the existing cost benchmark), and
  `miner::tests` **17/17 pass**. The latter includes the new V3 successful-publication control and missing/tampered
  opening refusals before execution/signing. These are scripted remote-node controls, separate from the real-chain
  two-fork API test above. Relevant legacy DNS anchors are walked through authenticated selected-parent openings,
  including anchors before the Rule E fork point. A mixed pre-Rule-E fork still fails closed rather than using a
  cross-activation comparator; no activation or complete migration acceptance is claimed.

### 2026-10-18 — validation merge

Merged validation 8478ec5dc, newer than required base 042e5e759. Incoming implementation wins all conflicts. Canonical op203 extras, op246 acceptance reads, remote rule_e reader, refund command and native job liability holds are retained. Codex op214 and its duplicate DTO are superseded. Paid-request generator and bounded carriage capture remain; adapters/tests/docs must be reconciled against the merged APIs before the next build. The c514deee4 runtime build and queued RPC test were deliberately interrupted at this source transition; neither is runtime evidence.
