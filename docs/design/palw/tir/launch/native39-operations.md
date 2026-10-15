# Owning39 native lane — operating procedures (RFC-0002 §II.14, row 14)

Status: **procedures for a dormant lane.** Nothing here is enabled on any preset. Every fence named below is `None` on mainnet,
testnet-10/11/12 and devnet/simnet, collapsed from `Some(never())`, hashed Some-only into both fork fingerprints, and refused
armed by its `validate_*` function. No line in this document, and no tool it names, reports PASS. All of the code it refers to
is unverified source (no build, no run) until the consolidated validation phase.

## 1. Activation policy (explicit, OFF)

| fence (Params field) | lane | refusal when armed |
| --- | --- | --- |
| `palw_native_execution_v39` | stage actions 41–72 (source, class, consent, Fund, Commit, filings, Final, GC 53, transport 54/55, release 71/72) | `validate_palw_native_execution_v39` |
| `palw_native_mining_v39` | stage actions 73–79 and the `PNR9` receipt-block carriage (algo 7) | `validate_palw_native_mining_v39` (always refuses) |
| Final27 + OPV (`palw_native_final_v27`, `palw_panel_free_v1`) | prerequisites of both | their own validators |
| RFC-0018 Gate C (`rfc0018_v1` mirror, table 136) | claim-level drawn approval for every mining claim | `validate_palw_rfc0018_v1` |
| ADR-0176 bond budget (`palw_bond_budget_v1`, export cap 0.51 `palw_bond_budget_export_cap_v1`) | Q/B/R/F, exposure, hold, allocation clip | `validate_palw_bond_budget_v1` |
| ADR-0179 model tier (`palw_model_tier_v1`, on integ/decisions-1015) | c_m = 1.0 genesis/annual-fork, 0.3 registered; applied once in A_m | `validate_palw_model_tier_v1` |

Arming requires a release that changes the presets and both fingerprints together, a drill that crosses the fence on the binary
being shipped, and the evidence in §6. A local qualified flag, an environment variable or a fixed PASS file is **not** an input
to any fold; there is no bypass. Below each fence the new kinds ride unjudged (A-2 uniform): the kernel route returns before any
state read or write (`palw_kernel_route_fold_v1.rs`, 73–79 also require `native_mining39`), and the header-form table admits
`PNR9` only past `palw_native_mining_v39` (`pow_layer0.rs` `NativeReceiptV39`).

## 2. Job lifecycle and retention

1. Producer registers source (67/68–70 metadata upload) and class (`prepare_source_class_v39`), consents (`prepare_source_consent_v39`),
   is Funded (`prepare_source_funding_v39`).
2. Commit: one carrier if the canonical Commit wire fits (`prepare_source_post_v39`), otherwise transport 54/55:
   `prepare_commit_parts_v39(raw, maximum)` then `prepare_commit_assembly_v39(job, group)`. Only the Funded producer, inside its
   commit window, at most two attempts; the first page of each attempt predebits the Metadata envelope.
3. Filings 48/50, Final 51/52 as before (unchanged by this row).
4. **Archive before collection.** Before anyone can submit action 53, every auditor who will need the job later runs
   `palw-native-final-audit39 --mode archive` against an anchored full-aux stream (`palw-native-audit-stream39`). It copies the
   live source, class and consent bodies into `NativeArchiveDirV39` (exclusive files keyed by original identity). After 53, the
   consent row is gone; the kind-31 tombstone carries its digests, so an archived consent is authenticated, never trusted.
5. **Collection 53** (`prepare_collect_v39(job)`, decode kind `source-collect-job39`): accepted only with a terminal, a credit,
   past `max(deadline, established + retention)`, no live owner or pending unit. Rows 16/17/23/28/84 survive for Masters and
   auditors; queue/physical leases close; the class hold `HOLD_JOB` is released.
6. **Retention release 71** (`prepare_retention_release_v39(class, id)`, decode kind `source-release39`, raw = kind byte ‖ id):
   only the recorded retention payer, and only once nothing live retains the slot. Immutable headers survive; freeing a slot
   never adds Q/B/R/F rights.
7. **Expired consent-hold collection 72** (`prepare_consent_hold_reap_v39(hold)`, decode kind `source-hold-reap39`): anyone may
   pay it. Candidates come from `native_expired_consent_holds_v39(rows, accept_daa)` over an authenticated stream; the fold
   re-reads the hold and refuses unless it is a canonical consent hold strictly past `until`.

Every frame above is a stage object signed by the submitting bond (ML-DSA-87, `PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1`); the
node checks the signer against the original owner rows for 48/50/54/55/68/69 before the fold, and the fold re-checks.

## 3. Independent audit

- Job: `palw-native-final-audit39 --mode job --id <job>` → `CheckedNativeJobAuditV39` from the stream plus the archive.
- Master: `--mode master --id <master>` → aggregate83 plus every component's job audit under one anchor.
- Anchor: the node exporter serves `NativeStateEnvelopeV39`; where rule E is in force at the parent DAA it serves the Rule-E V2
  leaf, otherwise the V1 leaf (`palw_accept.rs`, anchor39 branch). **Dependency:** the header-side root does not yet commit the
  V2 leaf (`palw_committed_state_root_v1` is V1-only; lane L2FC/ADR-0178). Rule E is dormant and refused armed everywhere, so a
  rule-E anchor fails closed at the auditor until that lane wires it.
- Receipts (A6/A8): `native_receipt_window_driver_v39.py` → `palw-native-receipt-audit39` → `native_receipt_window_audit_v39.py`.
  Two frozen windows, full denominators, named per-Master statuses; never sampled or shortened.
- Payout: `palw-native-redemption39` distinguishes settlement, pending, coinbase delivery and UTXO spend; when pending was
  consumed it requires the delivery witness.

## 4. Mining (73–79, dormant)

- 73 register (`prepare_mining_register_v39`): template Master with an original Final, registrant share ≤ policy maximum; the
  model's economic class retains `HOLD_MODEL`. Tier is ADR-0179's (0.3 for every registration); **no model-level exam,
  probation, renewal or promotion** (user decision 2026-10-15, round 5). A registration admits claims after the original
  maturity DAA only.
- 74 seat / 76 exit or retire (`prepare_mining_seat_v39`): seat stake is an ADR-0176 KernelRoute liability.
- 75 commit (`prepare_mining_commit_v39`): canonical work from real positions, final lengths and every component's compute;
  price clipped by the ADR-0176 plan (`plan_native_mining_budget_v39`, formula v2 `NativeFunded`); the claim enters RFC-0018
  Gate C as a `'V'` subject (commit-then-draw, m-of-k, inner-75 attestations, sweep).
- 77 issue (`prepare_mining_subject_v39(77, 0, claim)`): a receipt only when the claim is SecurityCertified and the native
  Gate C predicate holds; otherwise the claim closes and its budget is released.
- 78 is not a stage frame: a receipt quantum is spent by a `PNR9` header carriage (`native_receipt_spend_unsigned_v39`,
  `encode_native_receipt_carriage_v39`) through the existing receipt-spend entitlement/coinbase path (fold version 39).
- 79 collect: tombstones a closed claim or an exhausted/expired receipt.
- A private job's Final never counts: weight and coinbase come only through SecurityCertified receipts.

## 5. Eligibility exam

Superseded. The original row asked for a model eligibility exam; user decision 2026-10-15 (round 5) abolished model-level
verification for all models. Drawn-verifier approval exists only per claim, through RFC-0018 Gate C. RFC-0017 table 131 is not
consulted by any v39 path.

## 6. Before any activation proposal

Measured A6/A8 over two windows on the target ruleset; independent native Final and redemption replays; RFC-0018 Gate C,
ADR-0176 budget and ADR-0179 tier activation; the header-side Rule-E V2 commitment if rule E is part of the same release; the
initial seven models stay `NOT_RUN` until their own launch evidence exists (`initial-seven-models-v1.json`), with no forced
Active, no equal 1/7 split and no invented initial bonds; α = 1.5 and the 30-day eligible-bond allocation unchanged.
